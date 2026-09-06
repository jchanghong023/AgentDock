#!/usr/bin/env python3
"""Create a source-only ZIP with per-file SHA-256 integrity metadata."""
from __future__ import annotations
import argparse
import hashlib
from pathlib import Path, PurePosixPath
import zipfile

EXCLUDED_DIRS = {".git", "target", "dist", "__pycache__", ".venv"}
EXCLUDED_SUFFIXES = {".pyc", ".pyo", ".ttf", ".otf", ".woff", ".woff2"}


def allowed(path: PurePosixPath) -> bool:
    return not (set(path.parts) & EXCLUDED_DIRS) and path.suffix.lower() not in EXCLUDED_SUFFIXES


def package(root: Path, output: Path) -> dict:
    root, output = root.resolve(), output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    entries = []
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"refusing symlink: {path}")
        if path.is_file() and path.resolve() != output and allowed(PurePosixPath(path.relative_to(root).as_posix())):
            relative = path.relative_to(root).as_posix()
            if relative == "MANIFEST.sha256":
                continue
            data = path.read_bytes()
            entries.append((relative, data))
    manifest = "".join(f"{hashlib.sha256(data).hexdigest()}  {name}\n" for name, data in entries)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, data in entries + [("MANIFEST.sha256", manifest.encode())]:
            info = zipfile.ZipInfo("agentdock/" + name, date_time=(2026, 9, 6, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (0o100755 if name.endswith(".sh") else 0o100644) << 16
            archive.writestr(info, data)
    verify(output)
    digest = hashlib.sha256(output.read_bytes()).hexdigest()
    return {"zip": str(output), "files": len(entries) + 1, "bytes": output.stat().st_size, "sha256": digest}


def verify(path: Path) -> None:
    with zipfile.ZipFile(path) as archive:
        assert archive.testzip() is None, "ZIP CRC mismatch"
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise ValueError("duplicate archive entries")
        for name in names:
            item = PurePosixPath(name)
            if item.is_absolute() or ".." in item.parts or not name.startswith("agentdock/"):
                raise ValueError(f"unsafe archive path: {name}")
        manifest = archive.read("agentdock/MANIFEST.sha256").decode("utf-8")
        expected = set()
        for line in manifest.splitlines():
            digest, relative = line.split("  ", 1)
            name = "agentdock/" + relative
            expected.add(name)
            if hashlib.sha256(archive.read(name)).hexdigest() != digest:
                raise ValueError(f"checksum mismatch: {name}")
        if expected | {"agentdock/MANIFEST.sha256"} != set(names):
            raise ValueError("manifest does not match archive inventory")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    import json
    print(json.dumps(package(args.root, args.output), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
