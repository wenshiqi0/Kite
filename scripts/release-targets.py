"""Select release platforms from an annotated tag; ordinary pushes validate both."""
import json
import os
from pathlib import Path
import re
import subprocess

WINDOWS_MARKER = "<!-- Kite-Release-Targets: windows -->"
ALL_MARKER = "<!-- Kite-Release-Targets: all -->"


def select_targets(ref_type, notes=""):
    if ref_type != "tag":
        return {"android": True, "windows": True}
    markers = [line.strip() for line in notes.splitlines() if "Kite-Release-Targets:" in line]
    if not markers:
        return {"android": True, "windows": False}
    if markers == [ALL_MARKER]:
        return {"android": True, "windows": True}
    if markers != [WINDOWS_MARKER]:
        raise ValueError("Unknown or duplicate release-target marker")
    return {"android": False, "windows": True}


def main():
    ref_type = os.environ.get("GITHUB_REF_TYPE", "branch")
    notes = ""
    if ref_type == "tag":
        name = os.environ["GITHUB_REF_NAME"]
        if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-alpha\.[1-9]\d*)?", name):
            raise ValueError("Use a stable or Alpha release tag")
        ref = "refs/tags/" + name
        kind = subprocess.check_output(["git", "cat-file", "-t", ref], text=True).strip()
        if kind != "tag":
            raise ValueError("Releases require an annotated tag")
        notes = subprocess.check_output(["git", "for-each-ref", "--format=%(contents)", ref], text=True)
    targets = select_targets(ref_type, notes)
    if os.environ.get("GITHUB_OUTPUT"):
        with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
            for name, enabled in targets.items():
                output.write(f"{name}={str(enabled).lower()}\n")
    print(json.dumps(targets))


if __name__ == "__main__":
    main()
