#!/usr/bin/env python3
"""Read-only repository checks. NOT a Rust compiler, borrow checker, or GUI test.
Requires Python 3.11+. Pygments and PyYAML enable additional optional checks.
"""
from __future__ import annotations
import argparse
import ast
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib


REQUIRED = [
    "Cargo.toml", "rust-toolchain.toml", "src/main.rs", "src/lib.rs", "src/cli.rs",
    "src/model.rs", "src/paths.rs", "src/files.rs", "src/persistence.rs",
    "src/ui/app.rs", "src/ui/terminal_view.rs", "src/ui/virtual_list.rs", "src/ui/divider.rs",
    "src/terminal/engine.rs", "src/terminal/process.rs", "src/terminal/input.rs",
    "README.md", "LICENSE", "docs/COMPATIBILITY.md", "docs/ACCEPTANCE.md",
    ".github/workflows/ci.yml",
]


def rust_delimiters(source: str) -> list[str]:
    """Check punctuation after lexing literals/comments; this does NOT parse Rust."""
    from pygments.lexers import RustLexer
    from pygments.token import Punctuation, Error
    stack = []
    errors = []
    pairs = {"}": "{", ")": "(", "]": "["}
    for offset, kind, value in RustLexer().get_tokens_unprocessed(source):
        if kind in Error:
            errors.append(f"lexer error at byte/character offset {offset}: {value!r}")
        if kind not in Punctuation:
            continue
        for char in value:
            if char in "{([":
                stack.append((char, offset))
            elif char in pairs:
                if not stack or stack[-1][0] != pairs[char]:
                    errors.append(f"unmatched {char!r} at {offset}")
                else:
                    stack.pop()
    errors.extend(f"unclosed {char!r} at {offset}" for char, offset in stack)
    return errors


def inspect(root: Path) -> dict:
    root = root.resolve()
    passed, failed, skipped = [], [], []
    for name in REQUIRED:
        (passed if (root / name).is_file() else failed).append(f"required file: {name}")
    toml_files = list(root.rglob("*.toml"))
    for path in toml_files:
        try:
            tomllib.loads(path.read_text(encoding="utf-8"))
            passed.append(f"TOML parse: {path.relative_to(root)}")
        except (OSError, ValueError) as error:
            failed.append(f"TOML: {path}: {error}")
    try:
        cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
        iced = cargo["dependencies"]["iced"]
        features = set(iced["features"])
        assert iced["version"] == "=0.14.0"
        assert iced["default-features"] is False
        assert {"tiny-skia", "x11", "advanced", "markdown"} <= features
        assert not ({"wgpu", "wayland"} & features)
        assert cargo["dependencies"]["wezterm-term"]["rev"] == cargo["dependencies"]["termwiz"]["rev"]
        assert cargo["lints"]["rust"]["unsafe_code"] == "forbid"
        passed.append("declared CPU/X11 features, matched VT revisions, unsafe policy")
    except (OSError, ValueError, KeyError, AssertionError) as error:
        failed.append(f"dependency feature policy: {error!r}")
    # Validate example data only. verification/*.json contains this check's outputs.
    for path in sorted((root / "examples").glob("*.json")):
        try:
            json.loads(path.read_text(encoding="utf-8"))
            passed.append(f"JSON parse: {path.relative_to(root)}")
        except (OSError, ValueError) as error:
            failed.append(f"JSON: {path}: {error}")
    for path in sorted(root.rglob("*.py")):
        if "__pycache__" in path.parts:
            continue
        try:
            ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
            passed.append(f"Python syntax: {path.relative_to(root)}")
        except (OSError, SyntaxError) as error:
            failed.append(f"Python: {path}: {error}")
    try:
        import yaml
        for path in sorted((root / ".github/workflows").glob("*.yml")):
            config = yaml.load(path.read_text(encoding="utf-8"), Loader=yaml.BaseLoader)
            assert "jobs" in config and "on" in config
            passed.append(f"YAML parse: {path.relative_to(root)}")
    except ImportError:
        skipped.append("YAML parse: PyYAML not installed")
    except (ValueError, AssertionError) as error:
        failed.append(f"YAML: {error}")
    rust = sorted((root / "src").rglob("*.rs")) + sorted((root / "tests").glob("*.rs"))
    for path in rust:
        source = path.read_text(encoding="utf-8")
        if "\x00" in source:
            failed.append(f"NUL in Rust source: {path}")
        if re.search(r"\b(?:todo|unimplemented)!\s*\(", source):
            failed.append(f"unimplemented macro: {path}")
        # All external module declarations in this repository are top-level.
        for name in re.findall(r"(?:^|\n)\s*(?:pub\s+)?mod\s+(\w+)\s*;", source):
            base = path.parent if path.name in {"lib.rs", "main.rs", "mod.rs"} else path.with_suffix("")
            if not ((base / f"{name}.rs").is_file() or (base / name / "mod.rs").is_file()):
                failed.append(f"missing Rust module {name} from {path.relative_to(root)}")
        try:
            errors = rust_delimiters(source)
            if errors:
                failed.extend(f"Rust lexical delimiters {path.relative_to(root)}: {error}" for error in errors)
            else:
                passed.append(f"Rust lexical delimiters (NOT compilation): {path.relative_to(root)}")
        except ImportError:
            if "Rust lexical check: Pygments not installed" not in skipped:
                skipped.append("Rust lexical check: Pygments not installed")
    for path in sorted((root / "scripts").glob("*.sh")):
        if not shutil.which("bash"):
            skipped.append(f"bash -n: {path.name}: bash not installed")
            continue
        process = subprocess.run(["bash", "-n", str(path)], capture_output=True, text=True)
        if process.returncode:
            failed.append(f"bash syntax {path.name}: {process.stderr.strip()}")
        else:
            passed.append(f"bash -n: scripts/{path.name}")
    passed.append(f"Rust source inventory: {len(rust)} files; module-file links inspected")
    return {"scope": "source structure, lexical delimiters and helper syntax only", "passed": passed, "failed": failed, "skipped": skipped, "rust_compilation": "NOT_PERFORMED_BY_THIS_SCRIPT"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()
    result = inspect(args.root)
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    for label in ["passed", "failed", "skipped"]:
        for item in result[label]:
            print(f"{label.upper()}: {item}")
    print("These checks are NOT Rust compilation or application runtime validation.")
    return 1 if result["failed"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
