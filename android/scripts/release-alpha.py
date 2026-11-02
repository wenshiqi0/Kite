#!/usr/bin/env python3
"""Build, sign, push and publish an immutable Android Alpha to GitHub."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import sys
import zipfile

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


def apk_payload(apk):
    """Signing entries may change; application contents must remain identical."""
    with zipfile.ZipFile(apk) as archive:
        require(archive.testzip() is None, "APK ZIP CRC verification failed.")
        names = archive.namelist()
        require(len(names) == len(set(names)), "APK contains duplicate ZIP entries.")
        return {name: hashlib.sha256(archive.read(name)).hexdigest() for name in names
                if not re.fullmatch(r"META-INF/(?:MANIFEST\.MF|[^/]+\.(?:SF|RSA|DSA|EC))", name, re.IGNORECASE)}


def resign_apk(built, apk, tools, keystore, signing, *, v3=True):
    """Fresh SDK signatures using the existing certificate, without exposing passwords."""
    before = apk_payload(built)
    with tempfile.TemporaryDirectory(prefix="kite-apk-sign-") as temporary:
        aligned = Path(temporary) / "aligned.apk"
        run(str(tools / "zipalign"), "-f", "-P", "16", "4", str(built), str(aligned))
        environment = os.environ.copy()
        environment["KITE_APK_STORE_PASSWORD"] = signing["store.password"]
        environment["KITE_APK_KEY_PASSWORD"] = signing["key.password"]
        command = [str(tools / "apksigner"), "sign", "--ks", str(keystore),
                   "--ks-key-alias", signing["key.alias"],
                   "--ks-pass", "env:KITE_APK_STORE_PASSWORD",
                   "--key-pass", "env:KITE_APK_KEY_PASSWORD",
                   "--v1-signer-name", "CERT", "--v1-signing-enabled", "true",
                   "--v2-signing-enabled", "true", "--v3-signing-enabled", str(v3).lower(),
                   "--v4-signing-enabled", "false", "--out", str(apk), str(aligned)]
        subprocess.run(command, cwd=ROOT, env=environment, check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    require(apk_payload(apk) == before, "Re-signing changed the application payload.")


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


def inspect_apk(apk, tools, version, code, *, v3=True):
    badging = run(str(tools / "aapt2"), "dump", "badging", str(apk))
    package = next(line for line in badging.splitlines() if line.startswith("package:"))
    require(f"versionName='{version}'" in package and f"versionCode='{code}'" in package,
            "APK version does not match version.properties.")
    require("application-debuggable" not in badging, "Alpha APK must use the Release variant.")
    abi_line = next(line for line in badging.splitlines() if line.startswith("native-code:"))
    abis = re.findall(r"'([^']+)'", abi_line)
    signature = run(str(tools / "apksigner"), "verify", "--verbose", "--print-certs", str(apk))
    for scheme in ((2, 3) if v3 else (2,)):
        require(re.search(rf"^Verified using v{scheme} scheme .*: true$", signature, re.MULTILINE),
                f"Alpha APK must verify with signing scheme v{scheme}.")
    if not v3:
        require(re.search(r"^Verified using v3 scheme .*: false$", signature, re.MULTILINE),
                "Compatibility APK must avoid the v3 verification path.")
    certificate = re.search(r"certificate SHA-256 digest: ([0-9a-f]{64})", signature)
    require(certificate is not None, "APK signature verification failed.")
    expected = os.environ.get("KITE_EXPECTED_CERTIFICATE_SHA256")
    require(not expected or certificate[1].lower() == expected.lower(), "APK signing certificate changed.")
    # With minSdk >= 24, apksigner does not exercise the JAR path by default.
    legacy = run(str(tools / "apksigner"), "verify", "--verbose", "--print-certs",
                 "--min-sdk-version", "23", "--max-sdk-version", "23", str(apk))
    require(re.search(r"^Verified using v1 scheme .*: true$", legacy, re.MULTILINE),
            "Alpha APK must verify with signing scheme v1.")
    legacy_certificate = re.search(r"certificate SHA-256 digest: ([0-9a-f]{64})", legacy)
    require(legacy_certificate and legacy_certificate[1].lower() == certificate[1].lower(),
            "APK signature schemes must use the same certificate.")
    android16 = run(str(tools / "apksigner"), "verify", "--verbose", "--print-certs",
                    "--min-sdk-version", "36", "--max-sdk-version", "36", str(apk))
    selected = 3 if v3 else 2
    require(re.search(rf"^Verified using v{selected} scheme .*: true$", android16, re.MULTILINE),
            f"Android 16 must verify this APK using v{selected}.")
    android16_certificate = re.search(r"certificate SHA-256 digest: ([0-9a-f]{64})", android16)
    require(android16_certificate and android16_certificate[1].lower() == certificate[1].lower(),
            "Android 16 signing certificate changed.")
    run(str(tools / "zipalign"), "-c", "-P", "16", "4", str(apk))
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
    # PR merge commits and main build artifacts are validation, not publication.
    # The source/tag binding remains mandatory when launching or publishing CI.
    if not (args.check_only or args.build_only):
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
    compatibility_apk = output / f"kite-{version}-arm64-v8a-compat-v2.apk"
    linux_host = output / f"kite-host-{version}-linux-x86_64.tar.gz"
    manifest_file = output / "release-manifest.json"
    sums = output / "SHA256SUMS"
    assets = [apk, compatibility_apk, linux_host, manifest_file, sums]
    checksummed = [apk, compatibility_apk, linux_host, manifest_file]
    if args.build_only:
        require(sys.platform == "linux", "The synchronized Android/Linux Alpha build must run on Linux; launch trusted CI.")
        keystore = Path(os.environ.get("KITE_ALPHA_KEYSTORE", "~/.config/kite/signing/android-alpha.keystore")).expanduser()
        require(keystore.is_file(), "Configure the existing Alpha signing keystore outside the repository.")
        sdk = Path(os.environ.get("ANDROID_HOME", os.environ.get("ANDROID_SDK_ROOT", "")))
        candidates = [p for p in (sdk / "build-tools").glob("*")
                      if all((p / name).is_file() for name in ("aapt2", "apksigner", "zipalign"))]
        require(candidates, "Set ANDROID_HOME to an SDK containing aapt2, apksigner and zipalign.")
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
        resign_apk(built, apk, tools, keystore, signing)
        resign_apk(built, compatibility_apk, tools, keystore, signing, v3=False)
        require(apk_payload(apk) == apk_payload(compatibility_apk),
                "Compatibility signing changed application contents.")
        run("cargo", "build", "--release", "--locked", "-p", "kite-node", "-p", "kite-desktop", capture=False)
        target = Path(os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target")))
        host_binary = target / "release/kite-node"
        require(host_binary.is_file(), "Linux host binary was not produced.")
        run("bun", "run", str(ROOT / "packages/linux-host/src/package.ts"), "--binary", str(host_binary),
            "--output", str(linux_host), "--version", version, "--commit", head)
        clean_tree()
        abis, certificate = inspect_apk(apk, tools, version, code)
        require(abis == ["arm64-v8a"], "Update asset naming before publishing other ABIs.")
        compat_abis, compat_certificate = inspect_apk(compatibility_apk, tools, version, code, v3=False)
        require(compat_abis == abis and compat_certificate == certificate,
                "Compatibility APK must retain the exact ABI and signing identity.")
        manifest = {"versionName": version, "versionCode": code, "sourceCommit": head,
                    "variant": "release", "debuggable": False, "abis": abis,
                    "apk": apk.name, "bytes": apk.stat().st_size, "sha256": digest(apk),
                    "signingCertificateSha256": certificate, "signingSchemes": [1, 2, 3],
                    "apkSigningStep": "explicit-sdk-resign", "applicationPayloadUnchangedByResign": True,
                    "zipAlignmentKiB": 16, "androidLint": "passed"}
        manifest["compatibilityApk"] = {"asset": compatibility_apk.name, "bytes": compatibility_apk.stat().st_size,
            "sha256": digest(compatibility_apk), "signingCertificateSha256": compat_certificate,
            "signingSchemes": [1, 2], "android16VerifiedScheme": 2,
            "applicationPayloadMatchesPrimary": True, "zipAlignmentKiB": 16}
        manifest["linuxHostNode"] = {"asset": linux_host.name, "bytes": linux_host.stat().st_size,
            "sha256": digest(linux_host), "target": "x86_64-unknown-linux-gnu", "nodeApi": 1, "backendProtocol": 5}
        manifest_file.write_text(json.dumps(manifest, indent=2) + "\n")
        sums.write_text("".join(f"{digest(p)}  {p.name}\n" for p in checksummed))
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
    require(manifest["compatibilityApk"]["asset"] == compatibility_apk.name
            and manifest["compatibilityApk"]["sha256"] == digest(compatibility_apk)
            and manifest["compatibilityApk"]["signingCertificateSha256"] == manifest["signingCertificateSha256"]
            and manifest["compatibilityApk"]["signingSchemes"] == [1, 2]
            and manifest["compatibilityApk"]["android16VerifiedScheme"] == 2
            and apk_payload(apk) == apk_payload(compatibility_apk),
            "Compatibility APK provenance or contents do not match.")
    require(manifest["linuxHostNode"]["sha256"] == digest(linux_host)
            and manifest["linuxHostNode"]["asset"] == linux_host.name,
            "Linux host artifact provenance does not match this source tag.")
    require(sums.read_text() == "".join(f"{digest(p)}  {p.name}\n" for p in checksummed),
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
