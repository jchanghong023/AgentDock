#!/usr/bin/env python3
"""Read ELF version requirements without executing the binary. Not a full ABI guarantee."""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import re
import subprocess


def requirements(output: str) -> dict[str, list[str]]:
    result: dict[str, list[str]] = {}
    for family in ["GLIBC", "GLIBCXX", "CXXABI"]:
        versions = set(re.findall(rf"\b{family}_(\d+(?:\.\d+)+)\b", output))
        result[family] = sorted(versions, key=lambda s: tuple(map(int, s.split("."))))
    return result


def exceeds(versions: list[str], baseline: str) -> list[str]:
    floor = tuple(map(int, baseline.split(".")))
    return [v for v in versions if tuple(map(int, v.split("."))) > floor]


def audit(path: Path, floor: str) -> dict:
    raw = subprocess.run(["readelf", "--version-info", "--wide", str(path)], capture_output=True, text=True, check=True).stdout
    dynamic = subprocess.run(["readelf", "--dynamic", "--wide", str(path)], capture_output=True, text=True, check=True).stdout
    values = requirements(raw)
    bad = exceeds(values["GLIBC"], floor)
    return {"file": str(path), "baseline": floor, "requirements": values,
            "glibc_too_new": bad, "needed": re.findall(r"\(NEEDED\).*?\[(.*?)\]", dynamic),
            "note": "Repeat for every distributed .so. Matching GLIBC alone does not prove CentOS 7 compatibility."}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--glibc-max", default="2.17")
    args = parser.parse_args()
    try:
        result = audit(args.binary, args.glibc_max)
    except (OSError, subprocess.CalledProcessError) as error:
        print(json.dumps({"status": "ERROR", "error": str(error)}))
        return 2
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 1 if result["glibc_too_new"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
