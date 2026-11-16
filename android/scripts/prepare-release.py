#!/usr/bin/env python3
"""Prepare a release source commit and annotated tag; building stays in tagged CI."""
import argparse
import json
import os
from pathlib import Path
import subprocess
from release_version import read_version, version_key

ROOT = Path(__file__).resolve().parents[2]


def prepare(root, version, notes):
    def git(*args):
        return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True,
                              text=True).stdout.strip()

    requested = version_key(version)
    if git("symbolic-ref", "--short", "HEAD") != "main":
        raise RuntimeError("Prepare releases from main.")
    if git("status", "--porcelain", "--untracked-files=normal"):
        raise RuntimeError("Commit source changes before preparing a release.")
    properties = root / "android/version.properties"
    previous, current_code = read_version(properties)
    if requested <= version_key(previous):
        raise RuntimeError("The release version must increase; existing versions are immutable.")
    tag = "v" + version
    existing = subprocess.run(["git", "show-ref", "--verify", "--quiet", "refs/tags/" + tag],
                              cwd=root, check=False)
    if existing.returncode == 0:
        raise RuntimeError("This tag already exists. Rerun its build instead of replacing it.")
    if existing.returncode != 1:
        raise RuntimeError("Could not check existing release tags.")
    code = current_code + 1
    notes_file = root / "docs/release-notes" / (version + ".md")
    if notes_file.exists():
        raise RuntimeError("Release notes already exist; choose an unused version.")
    body = notes.strip() or (
        f"# Kite {version}\n\n"
        "Android 安装包、Linux 主机包和 Windows 客户端由同一源码标签的 GitHub CI 构建并验证。\n"
    )
    properties.write_text(f"versionName={version}\nversionCode={code}\n")
    notes_file.parent.mkdir(parents=True, exist_ok=True)
    notes_file.write_text(body + "\n")
    git("add", "android/version.properties", str(notes_file.relative_to(root)))
    git("commit", "-m", f"release: prepare {version}")
    git("tag", "-a", tag, "--cleanup=verbatim", "-F", str(notes_file))
    return {"version": version, "versionCode": code, "tag": tag,
            "commit": git("rev-parse", "HEAD")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()
    result = prepare(ROOT, args.version, os.environ.get("RELEASE_NOTES", ""))
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            for key in ("tag", "commit"):
                output.write(f"{key}={result[key]}\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error))
