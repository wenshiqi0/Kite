//! A per-session draft of explicitly selected host skills.
use super::{Client, Error};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: String,
}

#[derive(Clone, Debug, Default)]
pub struct SkillDraft {
    pub available: Vec<Skill>,
    pub selected: Vec<Skill>,
    pub loaded: bool,
    pub loading: bool,
    pub details: String,
}

impl SkillDraft {
    pub fn select(&mut self, path: &str, selected: bool) -> Result<(), Error> {
        if !selected {
            self.selected.retain(|s| s.path != path);
        } else if !self.selected.iter().any(|s| s.path == path) {
            let skill = self
                .available
                .iter()
                .find(|s| s.path == path)
                .ok_or(Error::Invalid("skill unavailable"))?;
            self.selected.push(skill.clone());
        }
        Ok(())
    }

    pub fn finish(&mut self, result: Result<(Vec<Skill>, String), Error>) {
        self.loading = false;
        match result {
            Ok((available, details)) => {
                self.selected = self
                    .selected
                    .iter()
                    .filter_map(|s| available.iter().find(|a| a.path == s.path).cloned())
                    .collect();
                self.available = available;
                self.loaded = true;
                self.details = details;
            }
            Err(error) => self.details = error.to_string(),
        }
    }

    pub fn sent(&mut self, sent: &[Skill]) {
        self.selected
            .retain(|s| !sent.iter().any(|old| old.path == s.path));
    }
}

impl Client {
    pub async fn list_skills(&self, cwd: &str) -> Result<(Vec<Skill>, String), Error> {
        if !cwd.starts_with('/') || cwd.chars().any(char::is_control) {
            return Err(Error::Invalid("skill working directory"));
        }
        let response = self
            .request("skills/list", json!({"cwds":[cwd],"forceReload":true}))
            .await?;
        decode(response, cwd)
    }
}

#[derive(Deserialize)]
struct Catalog {
    data: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    cwd: String,
    skills: Vec<Metadata>,
    errors: Vec<SkillError>,
}
#[derive(Deserialize)]
struct Metadata {
    name: String,
    description: String,
    path: String,
    enabled: bool,
}
#[derive(Deserialize)]
struct SkillError {
    message: String,
}

fn decode(response: Value, cwd: &str) -> Result<(Vec<Skill>, String), Error> {
    let catalog: Catalog = serde_json::from_value(response)?;
    let entry = catalog
        .data
        .into_iter()
        .find(|e| e.cwd == cwd)
        .ok_or(Error::Invalid(
            "skill working directory missing from response",
        ))?;
    let mut paths = BTreeSet::new();
    let mut skills: Vec<_> = entry
        .skills
        .into_iter()
        .filter(|s| {
            s.enabled
                && !s.name.is_empty()
                && !s.name.chars().any(|c| c.is_whitespace() || c.is_control())
                && s.path.starts_with('/')
                && !s.path.chars().any(char::is_control)
                && paths.insert(s.path.clone())
        })
        .map(|s| Skill {
            name: s.name,
            description: s.description,
            path: s.path,
        })
        .collect();
    skills.sort_by_cached_key(|s| (s.name.to_lowercase(), s.path.clone()));
    let details = entry
        .errors
        .into_iter()
        .take(8)
        .map(|e| e.message.chars().take(512).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    Ok((skills, details))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(name: &str) -> Skill {
        Skill {
            name: name.into(),
            description: format!("Use {name}"),
            path: format!("/skills/{name}/SKILL.md"),
        }
    }

    #[test]
    fn catalog_is_scoped_enabled_deduplicated_and_keeps_descriptions() {
        let wire = |name: &str, enabled: bool| json!({"name":name,"description":"A useful skill","path":format!("/skills/{name}/SKILL.md"),"enabled":enabled});
        let response = json!({"data":[
            {"cwd":"/other","skills":[wire("wrong-project",true)],"errors":[]},
            {"cwd":"/project","skills":[wire("z",true),wire("hidden",false),wire("a",true),wire("a",true),wire("bad name",true)],"errors":[{"message":"One skill could not load","path":"/bad"}]}
        ]});
        let (skills, details) = decode(response.clone(), "/project").unwrap();
        assert_eq!(
            skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["a", "z"]
        );
        assert_eq!(skills[0].description, "A useful skill");
        assert_eq!(details, "One skill could not load");
        assert!(decode(response, "/missing").is_err());
        assert!(decode(json!({}), "/project").is_err());
    }

    #[test]
    fn draft_selection_survives_catalog_failure_and_clears_after_send_or_removal() {
        let mut first = SkillDraft::default();
        first.finish(Ok((vec![skill("a"), skill("b")], String::new())));
        first.select(&skill("a").path, true).unwrap();
        first.select(&skill("a").path, true).unwrap();
        first.select(&skill("b").path, true).unwrap();
        assert_eq!(first.selected.len(), 2);
        assert!(SkillDraft::default().selected.is_empty());
        first.select(&skill("b").path, false).unwrap();
        first.finish(Err(Error::Invalid("offline")));
        assert_eq!(first.selected, [skill("a")]);
        first.finish(Ok((vec![skill("b")], String::new())));
        assert!(first.selected.is_empty());
        assert!(first.select(&skill("a").path, true).is_err());
        first.select(&skill("b").path, true).unwrap();
        first.sent(&[skill("b")]);
        assert!(first.selected.is_empty());
    }
}
