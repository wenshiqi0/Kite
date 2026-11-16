"""Publish only verified, immutable Windows artifacts through the shared Alpha CLI."""
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import zipfile


def verify_package(archive, checksum, version, commit):
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if checksum.read_text().split()[0] != digest:
        raise RuntimeError("Windows ZIP checksum mismatch")
    with zipfile.ZipFile(archive) as bundle:
        names = bundle.namelist()
        if len(names) != len(set(names)) or bundle.testzip() is not None:
            raise RuntimeError("Invalid Windows ZIP entries")
        manifest = json.loads(bundle.read("Kite/build-manifest.json"))
        if (manifest.get("source_commit") != commit or manifest.get("dirty") is not False
                or manifest.get("version") != version or manifest.get("target") != "win-x64"
                or manifest.get("built_on") != "linux" or manifest.get("host_role") is not False):
            raise RuntimeError("Windows artifact provenance does not match its source tag")
        expected = {"Kite/" + name for name in manifest["sha256"]} | {"Kite/build-manifest.json"}
        if set(names) != expected:
            raise RuntimeError("Windows package contains unrecorded files")
        for name, recorded in manifest["sha256"].items():
            if "/" in name or "\\" in name or name in {".", ".."}:
                raise RuntimeError("Invalid Windows package path")
            if hashlib.sha256(bundle.read("Kite/" + name)).hexdigest() != recorded:
                raise RuntimeError(f"Windows package file hash mismatch: {name}")
        deps = json.loads(bundle.read("Kite/Kite.deps.json"))
        if "Kite/" + version not in deps["libraries"]:
            raise RuntimeError("Windows application version does not match the release")
    return manifest, digest


def publish(repo, tag, version, commit, notes, output, *, run, require, release_info, wait_for_created_release):
    import os
    require(os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_REF_TYPE") == "tag",
            "Windows publication requires a tagged GitHub Actions build")
    run_id = os.environ["GITHUB_RUN_ID"]
    metadata = json.loads(run("gh", "api", f"repos/{repo}/actions/runs/{run_id}"))
    require(metadata["head_sha"] == commit and metadata["head_branch"] == tag and metadata["event"] == "push"
            and metadata["path"] == ".github/workflows/windows-client.yml", "Unexpected Windows build provenance")
    jobs = json.loads(run("gh", "api", f"repos/{repo}/actions/runs/{run_id}/jobs?per_page=100"))["jobs"]
    completed = {job["name"]: job["conclusion"] for job in jobs}
    require(all(completed.get(name) == "success" for name in ("cross-build", "windows-runtime")),
            "Linux cross-build and Windows runtime validation must pass before publication")
    run("git", "merge-base", "--is-ancestor", commit, "origin/main")
    output.mkdir(parents=True, exist_ok=True)
    originals = output / "ci-artifact"
    if not originals.exists():
        run("gh", "run", "download", run_id, "--repo", repo, "--name", "Kite-windows-x64", "--dir", str(originals))
    archives = list(originals.rglob("Kite-windows-x64.zip"))
    require(len(archives) == 1, "Expected exactly one verified Windows ZIP")
    original = archives[0]
    build, checksum = verify_package(original, Path(str(original) + ".sha256"), version, commit)
    archive = output / f"kite-{version}-windows-x64.zip"
    shutil.copyfile(original, archive)
    manifest = output / f"kite-{version}-windows-x64-manifest.json"
    manifest.write_text(json.dumps({"versionName": version, "sourceCommit": commit, "platform": "windows",
        "target": "win-x64", "builtOn": "linux", "clientOnly": True, "asset": archive.name,
        "bytes": archive.stat().st_size, "sha256": checksum, "workflowRun": int(run_id),
        "workflowUrl": metadata["html_url"], "windowsRuntimeValidation": "passed",
        "files": build["sha256"]}, indent=2) + "\n")
    digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
    sums = output / "SHA256SUMS-windows.txt"
    sums.write_text("".join(f"{digest(path)}  {path.name}\n" for path in (archive, manifest)))
    assets = [archive, manifest, sums]
    release = release_info(repo, tag)
    if release is None:
        run("gh", "release", "create", tag, "--repo", repo, "--verify-tag", "--target", "main",
            "--title", f"Kite {version} (Windows Alpha)", "--notes-file", str(notes),
            "--draft", "--prerelease", "--latest=false")
        release = wait_for_created_release(repo, tag)
    require(release and release["prerelease"], "Existing release must be an Alpha prerelease")
    names = {asset["name"] for asset in release["assets"]}
    missing = [path for path in assets if path.name not in names]
    require(release["draft"] or not missing, "Do not mutate an incomplete published release")
    if missing:
        run("gh", "release", "upload", tag, *(str(path) for path in missing), "--repo", repo)
    with tempfile.TemporaryDirectory(prefix="kite-windows-release-verify-") as scratch:
        for asset in assets:
            run("gh", "release", "download", tag, "--repo", repo, "--pattern", asset.name, "--dir", scratch)
            require(digest(Path(scratch) / asset.name) == digest(asset),
                    f"Remote {asset.name} differs; refuse to overwrite it")
    if release["draft"]:
        run("gh", "release", "edit", tag, "--repo", repo, "--draft=false", "--prerelease", "--latest=false")
    published = release_info(repo, tag)
    require(published and published["prerelease"] and not published["draft"], "Windows Alpha publication did not finish")
    print(published["html_url"], flush=True)
