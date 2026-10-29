#!/usr/bin/env python3
"""Build, sign, push and publish an immutable Android Alpha to GitHub."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def run(*args, capture=True):
    result = subprocess.run(args, cwd=ROOT, check=True, text=True,
                            stdout=subprocess.PIPE if capture else None,
                            stderr=subprocess.PIPE if capture else None)
    return result.stdout.strip() if capture else ""


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def clean_tree():
    require(not run("git", "status", "--porcelain", "--untracked-files=normal"),
            "Commit all source changes before packaging; the release must match its tag.")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def release_info(repo, tag):
    result = subprocess.run(["gh", "api", f"repos/{repo}/releases/tags/{tag}"],
                            cwd=ROOT, text=True, capture_output=True)
    if result.returncode and "404" in result.stderr:
        return None
    require(result.returncode == 0, result.stderr.strip())
    return json.loads(result.stdout)


def inspect_apk(apk, tools, version, code):
    badging = run(str(tools / "aapt2"), "dump", "badging", str(apk))
    package = next(line for line in badging.splitlines() if line.startswith("package:"))
    require(f"versionName='{version}'" in package and f"versionCode='{code}'" in package,
            "APK version does not match version.properties.")
    require("application-debuggable" not in badging, "Alpha APK must use the Release variant.")
    abi_line = next(line for line in badging.splitlines() if line.startswith("native-code:"))
    abis = re.findall(r"'([^']+)'", abi_line)
    signature = run(str(tools / "apksigner"), "verify", "--print-certs", str(apk))
    certificate = re.search(r"Signer #1 certificate SHA-256 digest: (\w+)", signature)
    require(certificate is not None, "APK signature verification failed.")
    return abis, certificate[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--notes-file", type=Path)
    parser.add_argument("--check-only", action="store_true")
    parser.add_argument("--build-only", action="store_true",
                        help="prepare verified assets without creating tags or writing to GitHub")
    args = parser.parse_args()
    props = dict(line.split("=", 1) for line in (ROOT / "android/version.properties").read_text().splitlines()
                 if line and not line.startswith("#"))
    version, code = props["versionName"], int(props["versionCode"])
    require(re.fullmatch(r"\d+\.\d+\.\d+-alpha\.[1-9]\d*", version) and code > 0,
            "Only X.Y.Z-alpha.N versions with a positive versionCode may be published.")
    clean_tree()
    head = run("git", "rev-parse", "HEAD")
    branch = run("git", "symbolic-ref", "--short", "HEAD")
    remote = run("git", "remote", "get-url", "origin")
    match = re.fullmatch(r"(?:git@github\.com:|https://github\.com/)([\w.-]+/[\w.-]+?)(?:\.git)?", remote)
    require(match is not None, "origin must identify the intended github.com repository.")
    repo = match[1]
    keystore = Path(os.environ.get("KITE_ALPHA_KEYSTORE", "~/.config/kite/signing/android-alpha.keystore")).expanduser()
    require(keystore.is_file(), "Configure the existing Alpha signing keystore outside the repository.")
    tag = "v" + version
    existing_tag = subprocess.run(["git", "rev-parse", "--verify", "--quiet", f"refs/tags/{tag}^{{commit}}"],
                                  cwd=ROOT, text=True, capture_output=True)
    require(existing_tag.returncode != 0 or existing_tag.stdout.strip() == head,
            "This version tag already belongs to a different commit; increment the Alpha version.")
    if args.check_only:
        print(json.dumps({"version": version, "versionCode": code, "commit": head,
                          "branch": branch, "repository": repo, "tag": tag}, indent=2))
        return
    require(args.notes_file and args.notes_file.is_file(), "Provide --notes-file with this release's changes and validation.")
    notes = args.notes_file.resolve()
    sdk = Path(os.environ.get("ANDROID_HOME", os.environ.get("ANDROID_SDK_ROOT", "")))
    candidates = [p for p in (sdk / "build-tools").glob("*")
                  if (p / "aapt2").is_file() and (p / "apksigner").is_file()]
    require(candidates, "Set ANDROID_HOME to an SDK with aapt2 and apksigner.")
    tools = max(candidates, key=lambda p: tuple(map(int, re.findall(r"\d+", p.name))))
    output = ROOT / "android/release-output" / tag
    output.mkdir(parents=True, exist_ok=True)
    apk = output / f"kite-{version}-arm64-v8a.apk"
    manifest_file = output / "release-manifest.json"
    cached = json.loads(manifest_file.read_text()) if manifest_file.exists() else None
    if cached:
        require(cached["sourceCommit"] == head and apk.is_file() and cached["sha256"] == digest(apk),
                "Cached output belongs to different source or content; use a new version.")
        print("Reusing the verified APK for this exact source commit.", flush=True)
    else:
        print(f"Building signed Release APK: {version}", flush=True)
        signing = {
            "store.file": str(keystore),
            "store.password": os.environ.get("KITE_ALPHA_STORE_PASSWORD", "android"),
            "key.alias": os.environ.get("KITE_ALPHA_KEY_ALIAS", "androiddebugkey"),
            "key.password": os.environ.get("KITE_ALPHA_KEY_PASSWORD", os.environ.get("KITE_ALPHA_STORE_PASSWORD", "android")),
        }
        command = [str(ROOT / "android/gradlew"), "-p", str(ROOT / "android"),
                   ":app:assembleRelease", ":app:lintRelease", "--no-daemon", "--console=plain"]
        command += shlex.split(os.environ.get("KITE_GRADLE_ARGS", ""))
        command += [f"-Pandroid.injected.signing.{key}={value}" for key, value in signing.items()]
        run(*command, capture=False)
        clean_tree()
        built = ROOT / "android/app/build/outputs/apk/release/app-release.apk"
        require(built.is_file(), "Signed release APK was not produced.")
        shutil.copy2(built, apk)
    abis, certificate = inspect_apk(apk, tools, version, code)
    require(abis == ["arm64-v8a"], "Update the asset naming policy before publishing different ABIs.")
    manifest = {"versionName": version, "versionCode": code, "sourceCommit": head,
                "variant": "release", "debuggable": False, "abis": abis,
                "apk": apk.name, "bytes": apk.stat().st_size, "sha256": digest(apk),
                "signingCertificateSha256": certificate, "androidLint": "passed"}
    manifest_file.write_text(json.dumps(manifest, indent=2) + "\n")
    sums = output / "SHA256SUMS"
    sums.write_text("".join(f"{digest(p)}  {p.name}\n" for p in [apk, manifest_file]))
    assets = [apk, manifest_file, sums]
    clean_tree()
    require(run("git", "rev-parse", "HEAD") == head, "Source commit changed during packaging.")
    if args.build_only:
        print(str(output), flush=True)
        return
    if existing_tag.returncode != 0:
        run("git", "tag", "-a", tag, "-m", f"Kite {version}")
    print(f"Pushing source and tag to {repo}", flush=True)
    run("git", "push", "--atomic", "origin", f"HEAD:refs/heads/{branch}", f"refs/tags/{tag}", capture=False)
    release = release_info(repo, tag)
    if release is None:
        run("gh", "release", "create", tag, "--repo", repo, "--verify-tag", "--target", head,
            "--title", f"Kite {version} (Alpha)", "--notes-file", str(notes),
            "--draft", "--prerelease", "--latest=false")
        release = release_info(repo, tag)
    require(release and release["prerelease"], "Existing release is not an Alpha prerelease.")
    names = {a["name"] for a in release["assets"]}
    missing = [p for p in assets if p.name not in names]
    require(release["draft"] or not missing, "Published release is incomplete; do not mutate it.")
    if missing:
        run("gh", "release", "upload", tag, *(str(p) for p in missing), "--repo", repo)
    print("Downloading and verifying all release assets before publication.", flush=True)
    with tempfile.TemporaryDirectory(prefix="kite-alpha-verify-") as scratch:
        for asset in assets:
            run("gh", "release", "download", tag, "--repo", repo, "--pattern", asset.name, "--dir", scratch)
            require(digest(Path(scratch) / asset.name) == digest(asset),
                    f"Remote {asset.name} differs; refuse to replace an existing asset.")
    if release["draft"]:
        run("gh", "release", "edit", tag, "--repo", repo, "--draft=false", "--prerelease", "--latest=false")
    published = release_info(repo, tag)
    require(published and published["prerelease"] and not published["draft"], "Release did not become a published Alpha.")
    (output / "publication.json").write_text(json.dumps({"url": published["html_url"], "tag": tag,
                                                        "sourceCommit": head, "sha256": digest(apk)}, indent=2) + "\n")
    print(published["html_url"], flush=True)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError) as error:
        detail = error.stderr.strip() if isinstance(error, subprocess.CalledProcessError) and error.stderr else str(error)
        raise SystemExit(detail)
