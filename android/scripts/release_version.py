"""Version rules shared by GitHub release preparation and publication."""
import re

VERSION = re.compile(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-alpha\.([1-9]\d*))?")


def version_key(version):
    match = VERSION.fullmatch(version)
    if not match:
        raise RuntimeError("Use X.Y.Z for a stable release or X.Y.Z-alpha.N for an Alpha.")
    major, minor, patch, alpha = match.groups()
    return (int(major), int(minor), int(patch), 0 if alpha else 1, int(alpha or 0))


def prerelease(version):
    return version_key(version)[3] == 0


def read_version(path):
    props = dict(line.split("=", 1) for line in path.read_text().splitlines()
                 if line and not line.startswith("#"))
    version, code = props["versionName"], int(props["versionCode"])
    version_key(version)
    if code <= 0:
        raise RuntimeError("versionCode must be positive.")
    return version, code
