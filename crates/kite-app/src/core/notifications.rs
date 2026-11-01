use super::*;
use kite_agent::notifications::{Kind, Page};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentNotice {
    pub id: String,
    pub host_id: String,
    pub session_id: String,
    pub title: String,
    pub kind: Kind,
    pub created_ms: i64,
    pub read: bool,
    pub alerted: bool,
    verified: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct NotificationInbox {
    version: u32,
    cursors: BTreeMap<String, String>,
    items: Vec<AgentNotice>,
}
impl Default for NotificationInbox {
    fn default() -> Self {
        Self {
            version: 1,
            cursors: BTreeMap::new(),
            items: Vec::new(),
        }
    }
}
impl NotificationInbox {
    pub(super) fn valid(&self) -> bool {
        self.version == 1
            && self.items.len() <= 200
            && self.cursors.len() <= 1024
            && self.items.iter().all(|item| {
                item.id
                    .strip_prefix(&format!("{}/", item.host_id))
                    .is_some_and(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()))
                    && item.host_id.len() <= 128
                    && item.session_id.len() <= 128
                    && item.title.len() <= 2048
            })
            && self
                .cursors
                .iter()
                .all(|(host, cursor)| host.len() <= 128 && cursor.len() <= 192)
    }
    fn ingest(&mut self, host: &str, page: Page) -> bool {
        let old = self.cursors.get(host).cloned();
        let mut changed = false;
        for event in page.events {
            if event.id.len() != 64 || event.session_id.len() > 128 || event.title.len() > 2048 {
                continue;
            }
            let id = format!("{host}/{}", event.id);
            if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
                if !item.verified {
                    item.session_id = event.session_id;
                    item.title = event.title;
                    item.verified = true;
                    changed = true;
                }
                continue;
            }
            self.items.push(AgentNotice {
                id,
                host_id: host.into(),
                session_id: event.session_id,
                title: event.title,
                kind: event.kind,
                created_ms: event.created_ms,
                read: false,
                alerted: false,
                verified: true,
            });
            changed = true;
        }
        self.items.sort_by_key(|item| item.created_ms);
        if self.items.len() > 200 {
            self.items.drain(..self.items.len() - 200);
        }
        self.cursors.insert(host.into(), page.cursor.clone());
        changed || old.as_deref() != Some(&page.cursor)
    }
}
impl AppCore {
    pub fn notification_host(&self, id: &str) -> Result<String, AppError> {
        let (host, event) = id
            .split_once('/')
            .ok_or_else(|| AppError::State("invalid notification target".into()))?;
        if event.len() != 64 || !event.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(AppError::State("invalid notification identity".into()));
        }
        self.host(host)?;
        Ok(host.into())
    }
    pub async fn load_notification(self: &Arc<Self>, id: String) -> Result<AgentNotice, AppError> {
        let core = self.clone();
        dispatch(async move {
            let host = core.notification_host(&id)?;
            if let Some(item) = core
                .notifications()
                .into_iter()
                .find(|item| item.id == id && item.verified)
            {
                return Ok(item);
            }
            let connection = core.connection()?;
            if connection.host.id != host {
                return Err(AppError::State(
                    "notification target is not connected".into(),
                ));
            }
            let raw = id.split_once('/').map(|pair| pair.1).unwrap_or_default();
            let command = format!(
                "\"$HOME/.local/bin/kite-node\" notification-get {}",
                composer::quote(raw)
            );
            let output = connection.ssh.exec(&command, &[]).await?;
            if output.exit_status != Some(0) {
                return Err(AppError::State("notification could not be verified".into()));
            }
            let event: Option<kite_agent::notifications::Event> =
                serde_json::from_slice(&output.stdout)
                    .map_err(|_| AppError::State("invalid notification response".into()))?;
            let event = event
                .filter(|event| event.id == raw && event.valid())
                .ok_or_else(|| AppError::State("notification expired or unavailable".into()))?;
            let item = AgentNotice {
                id,
                host_id: host,
                session_id: event.session_id,
                title: event.title,
                kind: event.kind,
                created_ms: event.created_ms,
                read: false,
                alerted: true,
                verified: true,
            };
            let mut inbox = lock(&core.notifications);
            let mut next = inbox.clone();
            next.items.retain(|old| old.id != item.id);
            next.items.push(item.clone());
            if next.items.len() > 200 {
                next.items.remove(0);
            }
            core.save_notifications(&next)?;
            *inbox = next;
            drop(inbox);
            core.update(|s| s.notification_revision = s.notification_revision.wrapping_add(1));
            Ok(item)
        })
        .await
    }
    pub async fn register_push(
        self: &Arc<Self>,
        device_id: String,
        token: Option<String>,
        project_id: String,
    ) -> Result<bool, AppError> {
        let core = self.clone();
        dispatch(async move {
            if device_id.is_empty()||device_id.len()>128||!device_id.bytes().all(|b|b.is_ascii_alphanumeric()||b"_-".contains(&b))||project_id.len()>63||token.as_ref().is_some_and(|t|t.len()>4096){return Err(AppError::State("invalid push registration".into()))}
            let connection=core.connection()?;
            let mut bytes=serde_json::to_vec(&serde_json::json!({"device_id":device_id,"host_id":connection.host.id,"token":token,"project_id":project_id})).map_err(|e|AppError::State(e.to_string()))?;bytes.push(b'\n');
            let output=connection.ssh.exec("\"$HOME/.local/bin/kite-node\" push-register",&bytes).await?;
            if output.exit_status!=Some(0){return Err(AppError::State("host push registration unavailable; check matching host version".into()))}
            let status:serde_json::Value=serde_json::from_slice(&output.stdout).map_err(|_|AppError::State("invalid push registration response".into()))?;
            Ok(status["configured"]==true)
        }).await
    }
    /// A system push is a wake-up hint. Its session is resolved over pinned SSH on tap.
    pub async fn receive_push(
        self: &Arc<Self>,
        id: String,
        kind: Kind,
        session_id: String,
        created_ms: i64,
    ) -> Result<AgentNotice, AppError> {
        let core = self.clone();
        dispatch(async move {
            let host = core.notification_host(&id)?;
            if session_id.is_empty() || session_id.len() > 128 || created_ms < 0 {
                return Err(AppError::State("invalid push event metadata".into()));
            }
            let mut inbox = lock(&core.notifications);
            if let Some(item) = inbox.items.iter().find(|item| item.id == id) {
                return Ok(item.clone());
            }
            let mut next = inbox.clone();
            let item = AgentNotice {
                id,
                host_id: host,
                session_id,
                title: "Kite".into(),
                kind,
                created_ms,
                read: false,
                alerted: false,
                verified: false,
            };
            next.items.push(item.clone());
            next.items.sort_by_key(|item| item.created_ms);
            if next.items.len() > 200 {
                next.items.remove(0);
            }
            core.save_notifications(&next)?;
            *inbox = next;
            drop(inbox);
            core.update(|s| s.notification_revision = s.notification_revision.wrapping_add(1));
            Ok(item)
        })
        .await
    }
    pub fn notifications(&self) -> Vec<AgentNotice> {
        lock(&self.notifications)
            .items
            .iter()
            .rev()
            .cloned()
            .collect()
    }
    pub async fn acknowledge_notification(
        self: &Arc<Self>,
        id: String,
        read: bool,
        alerted: bool,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let mut inbox = lock(&core.notifications);
            let mut next = inbox.clone();
            let item = next
                .items
                .iter_mut()
                .find(|item| item.id == id)
                .ok_or_else(|| AppError::State("notification unavailable".into()))?;
            if (!read || item.read) && (!alerted || item.alerted) {
                return Ok(());
            }
            item.read |= read;
            item.alerted |= alerted;
            core.save_notifications(&next)?;
            *inbox = next;
            drop(inbox);
            core.update(|s| s.notification_revision = s.notification_revision.wrapping_add(1));
            Ok(())
        })
        .await
    }
    fn save_notifications(&self, inbox: &NotificationInbox) -> Result<(), AppError> {
        if !self.notification_ready.load(Ordering::SeqCst) {
            return Err(AppError::State(
                "notification storage unavailable; existing file preserved".into(),
            ));
        }
        let store = lock(&self.store);
        let store = store
            .as_ref()
            .ok_or_else(|| AppError::State("core is not initialized".into()))?;
        store.write_document("notifications", inbox)?;
        Ok(())
    }
    pub(super) fn start_notification_sync(
        self: &Arc<Self>,
        connection: Arc<Connected>,
        generation: u64,
    ) {
        if let Some(job) = lock(&self.notification_job).take() {
            job.abort();
        }
        if !self.notification_ready.load(Ordering::SeqCst) {
            return;
        }
        let core = self.clone();
        *lock(&self.notification_job) = Some(tokio::spawn(async move {
            loop {
                if core.generation.load(Ordering::SeqCst) != generation
                    || connection.ssh.is_closed()
                {
                    return;
                }
                let after = lock(&core.notifications)
                    .cursors
                    .get(&connection.host.id)
                    .cloned();
                let command = format!(
                    "\"$HOME/.local/bin/kite-node\" notifications{}",
                    after
                        .as_ref()
                        .map(|after| format!(" --after {}", composer::quote(after)))
                        .unwrap_or_default()
                );
                let result=async{
                    let output=connection.ssh.exec(&command,&[]).await?;
                    if output.exit_status!=Some(0){return Err(AppError::State("host notification service unavailable; check the matching host version".into()))}
                    let page:Page=serde_json::from_slice(&output.stdout).map_err(|_|AppError::State("invalid notification page".into()))?;
                    if !page.valid(){return Err(AppError::State("invalid notification page identity, ordering or limits".into()))}
                    if core.generation.load(Ordering::SeqCst)!=generation{return Ok(())}
                    let mut inbox=lock(&core.notifications);let mut next=inbox.clone();
                    if next.ingest(&connection.host.id,page){core.save_notifications(&next)?;*inbox=next;drop(inbox);core.update(|s|s.notification_revision=s.notification_revision.wrapping_add(1));}
                    Ok::<_,AppError>(())
                }.await;
                let problem = result
                    .err()
                    .map(|error| error.to_string().chars().take(256).collect::<String>());
                if core.generation.load(Ordering::SeqCst) == generation
                    && core.snapshot().notification_problem != problem
                {
                    core.update(|s| s.notification_problem = problem);
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kite_agent::notifications::Event;
    #[test]
    fn reconnect_pages_do_not_duplicate_or_mark_notifications_read() {
        let mut inbox = NotificationInbox::default();
        let event = Event {
            id: "a".repeat(64),
            sequence: 1,
            created_ms: 10,
            session_id: "session".into(),
            title: "project".into(),
            kind: Kind::Completed,
            source: "source".into(),
        };
        let page = Page {
            node_id: "node".into(),
            cursor: "node:1".into(),
            reset: false,
            more: false,
            events: vec![event],
        };
        assert!(inbox.ingest("host", page.clone()));
        assert!(!inbox.ingest("host", page));
        assert_eq!(inbox.items.len(), 1);
        assert!(!inbox.items[0].read);
        assert!(!inbox.items[0].alerted);
        let recovered: NotificationInbox =
            serde_json::from_slice(&serde_json::to_vec(&inbox).unwrap()).unwrap();
        assert!(recovered.valid());
        assert_eq!(recovered.cursors["host"], "node:1");
    }
}
