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
        # GitHub's tag endpoint excludes draft releases. Owners can recover the
        # draft through the paginated release list after a partial publication.
        pages = json.loads(run("gh", "api", "--paginate", "--slurp", f"repos/{repo}/releases?per_page=100"))
        return next((release for page in pages for release in page if release["tag_name"] == tag), None)
    require(result.returncode == 0, result.stderr.strip())
    return json.loads(result.stdout)


def wait_for_created_release(repo, tag):
    # A newly created draft can briefly be absent from GitHub's release list.
    # Retry reads only; never replay creation, uploads or publication.
    import time
    for delay in (0, 1, 2, 4, 8):
        if delay:
            time.sleep(delay)
        release = release_info(repo, tag)
        if release is not None:
            return release
    raise RuntimeError("Created Alpha draft is not visible yet; rerun the publish job to recover it.")


def inspect_apk(apk, tools, version, code):
    badging = run(str(tools / "aapt2"), "dump", "badging", str(apk))
    package = next(line for line in badging.splitlines() if line.startswith("package:"))
    require(f"versionName='{version}'" in package and f"versionCode='{code}'" in package,
            "APK version does not match version.properties.")
    require("application-debuggable" not in badging, "Alpha APK must use the Release variant.")
    abi_line = next(line for line in badging.splitlines() if line.startswith("native-code:"))
    abis = re.findall(r"'([^']+)'", abi_line)
    signature = run(str(tools / "apksigner"), "verify", "--print-certs", str(apk))
    certificate = re.search(r"certificate SHA-256 digest: ([0-9a-f]{64})", signature)
    require(certificate is not None, "APK signature verification failed.")
    expected = os.environ.get("KITE_EXPECTED_CERTIFICATE_SHA256")
    require(not expected or certificate[1].lower() == expected.lower(), "APK signing certificate changed.")
    return abis, certificate[1]


def launch_ci(repo, tag, head, branch, notes, existing_tag):
    import time
    require(branch, "Use a source branch when launching a release.")
    require(notes.is_file(), "Provide release notes with --notes-file.")
    if existing_tag.returncode != 0:
        run("git", "tag", "-a", tag, "-F", str(notes))
    run("git", "push", "--atomic", "origin", f"HEAD:refs/heads/{branch}", f"refs/tags/{tag}", capture=False)
    print(f"Waiting for the tagged build: https://github.com/{repo}/actions", flush=True)
    for _ in range(30):
        runs = json.loads(run("gh", "run", "list", "--repo", repo, "--workflow", "android-release.yml",
                              "--event", "push", "--commit", head, "--limit", "30",
                              "--json", "databaseId,headBranch"))
        matches = [r for r in runs if r["headBranch"] == tag]
        if matches:
            run("gh", "run", "watch", str(matches[0]["databaseId"]), "--repo", repo, "--exit-status", capture=False)
            release = release_info(repo, tag)
            require(release and release["prerelease"] and not release["draft"], "CI did not publish the Alpha.")
            print(release["html_url"], flush=True)
            return
        time.sleep(2)
    raise RuntimeError("Tag was pushed, but its workflow did not start; inspect GitHub Actions.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--notes-file", type=Path)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check-only", action="store_true")
    mode.add_argument("--build-only", action="store_true", help="build and verify assets without publishing")
    mode.add_argument("--publish-only", action="store_true", help="publish the verified assets from the tagged CI build")
    args = parser.parse_args()
    props = dict(line.split("=", 1) for line in (ROOT / "android/version.properties").read_text().splitlines()
                 if line and not line.startswith("#"))
    version, code = props["versionName"], int(props["versionCode"])
    require(re.fullmatch(r"\d+\.\d+\.\d+-alpha\.[1-9]\d*", version) and code > 0,
            "Only X.Y.Z-alpha.N versions with a positive versionCode may be published.")
    clean_tree()
    head = run("git", "rev-parse", "HEAD")
    ref = subprocess.run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], cwd=ROOT, text=True, capture_output=True)
    branch = ref.stdout.strip()
    remote = run("git", "remote", "get-url", "origin")
    match = re.fullmatch(r"(?:git@github\.com:|https://github\.com/)([\w.-]+/[\w.-]+?)(?:\.git)?", remote)
    require(match is not None, "origin must identify the intended github.com repository.")
    repo, tag = match[1], "v" + version
    existing_tag = subprocess.run(["git", "rev-parse", "--verify", "--quiet", f"refs/tags/{tag}^{{commit}}"],
                                  cwd=ROOT, text=True, capture_output=True)
    require(existing_tag.returncode != 0 or existing_tag.stdout.strip() == head,
            "This tag belongs to another commit; increment the Alpha version.")
    if os.environ.get("GITHUB_REF_TYPE") == "tag":
        require(os.environ.get("GITHUB_REF_NAME") == tag, "Git tag must match the APK version.")
    if args.check_only:
        print(json.dumps({"version": version, "versionCode": code, "commit": head,
                          "branch": branch or "detached", "repository": repo, "tag": tag}, indent=2))
        return
    notes = (args.notes_file or ROOT / "docs/release-notes" / f"{version}.md").resolve()
    if not args.build_only and not args.publish_only:
        launch_ci(repo, tag, head, branch, notes, existing_tag)
        return
    output = ROOT / "android/release-output" / tag
    apk = output / f"kite-{version}-arm64-v8a.apk"
    manifest_file = output / "release-manifest.json"
    sums = output / "SHA256SUMS"
    assets = [apk, manifest_file, sums]
    if args.build_only:
        keystore = Path(os.environ.get("KITE_ALPHA_KEYSTORE", "~/.config/kite/signing/android-alpha.keystore")).expanduser()
        require(keystore.is_file(), "Configure the existing Alpha signing keystore outside the repository.")
        sdk = Path(os.environ.get("ANDROID_HOME", os.environ.get("ANDROID_SDK_ROOT", "")))
        candidates = [p for p in (sdk / "build-tools").glob("*")
                      if (p / "aapt2").is_file() and (p / "apksigner").is_file()]
        require(candidates, "Set ANDROID_HOME to an SDK containing aapt2 and apksigner.")
        tools = max(candidates, key=lambda p: tuple(map(int, re.findall(r"\d+", p.name))))
        signing = {
            "store.file": str(keystore),
            "store.password": os.environ.get("KITE_ALPHA_STORE_PASSWORD", "android"),
            "key.alias": os.environ.get("KITE_ALPHA_KEY_ALIAS", "androiddebugkey"),
            "key.password": os.environ.get("KITE_ALPHA_KEY_PASSWORD", os.environ.get("KITE_ALPHA_STORE_PASSWORD", "android")),
        }
        print(f"Building signed Release APK: {version}", flush=True)
        command = [str(ROOT / "android/gradlew"), "-p", str(ROOT / "android"),
                   ":app:assembleRelease", ":app:lintRelease", ":app:testReleaseUnitTest", "--no-daemon", "--console=plain"]
        command += shlex.split(os.environ.get("KITE_GRADLE_ARGS", ""))
        command += [f"-Pandroid.injected.signing.{key}={value}" for key, value in signing.items()]
        run(*command, capture=False)
        clean_tree()
        built = ROOT / "android/app/build/outputs/apk/release/app-release.apk"
        require(built.is_file(), "Signed release APK was not produced.")
        output.mkdir(parents=True, exist_ok=True)
        shutil.copy2(built, apk)
        abis, certificate = inspect_apk(apk, tools, version, code)
        require(abis == ["arm64-v8a"], "Update asset naming before publishing other ABIs.")
        manifest = {"versionName": version, "versionCode": code, "sourceCommit": head,
                    "variant": "release", "debuggable": False, "abis": abis,
                    "apk": apk.name, "bytes": apk.stat().st_size, "sha256": digest(apk),
                    "signingCertificateSha256": certificate, "androidLint": "passed"}
        manifest_file.write_text(json.dumps(manifest, indent=2) + "\n")
        sums.write_text("".join(f"{digest(p)}  {p.name}\n" for p in [apk, manifest_file]))
        print(str(output), flush=True)
        return
    require(os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_REF_TYPE") == "tag",
            "Publication must use the verified tagged GitHub Actions build.")
    require(existing_tag.returncode == 0 and notes.is_file(), "An existing source tag and release notes are required.")
    require(all(p.is_file() for p in assets), "The verified build artifacts are incomplete.")
    manifest = json.loads(manifest_file.read_text())
    require(manifest["sourceCommit"] == head and manifest["versionName"] == version
            and manifest["versionCode"] == code and manifest["sha256"] == digest(apk)
            and manifest["variant"] == "release" and manifest["debuggable"] is False,
            "Artifact provenance does not match this source tag.")
    require(sums.read_text() == "".join(f"{digest(p)}  {p.name}\n" for p in [apk, manifest_file]),
            "Artifact checksums do not match.")
    release = release_info(repo, tag)
    if release is None:
        run("gh", "release", "create", tag, "--repo", repo, "--verify-tag", "--target", head,
            "--title", f"Kite {version} (Alpha)", "--notes-file", str(notes),
            "--draft", "--prerelease", "--latest=false")
        release = wait_for_created_release(repo, tag)
    require(release and release["prerelease"], "Existing release is not an Alpha prerelease.")
    names = {a["name"] for a in release["assets"]}
    missing = [p for p in assets if p.name not in names]
    require(release["draft"] or not missing, "Do not mutate an incomplete published release.")
    if missing:
        run("gh", "release", "upload", tag, *(str(p) for p in missing), "--repo", repo)
    with tempfile.TemporaryDirectory(prefix="kite-alpha-verify-") as scratch:
        for asset in assets:
            run("gh", "release", "download", tag, "--repo", repo, "--pattern", asset.name, "--dir", scratch)
            require(digest(Path(scratch) / asset.name) == digest(asset),
                    f"Remote {asset.name} differs; refuse to replace an existing asset.")
    if release["draft"]:
        run("gh", "release", "edit", tag, "--repo", repo, "--draft=false", "--prerelease", "--latest=false")
    published = release_info(repo, tag)
    require(published and published["prerelease"] and not published["draft"], "Alpha publication was not completed.")
    print(published["html_url"], flush=True)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError) as error:
        detail = (error.stderr.strip() if error.stderr else f"Command failed with exit code {error.returncode}; inspect its output.") if isinstance(error, subprocess.CalledProcessError) else str(error)
        raise SystemExit(detail)
