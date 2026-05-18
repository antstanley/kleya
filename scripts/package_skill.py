#!/usr/bin/env python3
"""Package a Claude Code skill folder into a distributable .skill zip.

The .skill format is a zip of the skill folder contents. Validation: SKILL.md
must exist at the root of the skill folder. The evals/ directory is excluded
from the package — it is dev-time material (test prompts, run outputs), not
something end users install.

Usage:
    package_skill.py <skill-folder> [-o <output-dir>]

Stdlib only — no third-party deps so CI doesn't need a Python env beyond the
runner's preinstalled `python3`.
"""

from __future__ import annotations

import argparse
import sys
import zipfile
from pathlib import Path

EXCLUDE_DIRS = {"__pycache__", "node_modules", ".git"}
EXCLUDE_FILES = {".DS_Store"}
EXCLUDE_GLOBS = ("*.pyc",)
ROOT_EXCLUDE_DIRS = {"evals"}


def should_exclude(rel: Path) -> bool:
    parts = rel.parts
    if any(p in EXCLUDE_DIRS for p in parts):
        return True
    if len(parts) > 1 and parts[1] in ROOT_EXCLUDE_DIRS:
        return True
    if rel.name in EXCLUDE_FILES:
        return True
    return any(rel.match(g) for g in EXCLUDE_GLOBS)


def package(skill_dir: Path, output_dir: Path) -> Path:
    if not (skill_dir / "SKILL.md").is_file():
        raise SystemExit(f"error: SKILL.md not found in {skill_dir}")

    output_dir.mkdir(parents=True, exist_ok=True)
    out_path = output_dir / f"{skill_dir.name}.skill"

    with zipfile.ZipFile(out_path, "w", zipfile.ZIP_DEFLATED) as zf:
        for path in sorted(skill_dir.rglob("*")):
            if not path.is_file():
                continue
            rel = path.relative_to(skill_dir.parent)
            if should_exclude(rel):
                continue
            zf.write(path, rel)

    return out_path


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("skill", type=Path, help="Path to the skill folder")
    ap.add_argument(
        "-o", "--output-dir", type=Path, default=Path("."),
        help="Directory to write the .skill file into (default: current dir)",
    )
    args = ap.parse_args()

    skill_dir = args.skill.resolve()
    if not skill_dir.is_dir():
        print(f"error: not a directory: {skill_dir}", file=sys.stderr)
        return 2

    out = package(skill_dir, args.output_dir.resolve())
    print(f"wrote {out} ({out.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
