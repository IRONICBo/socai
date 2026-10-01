#!/usr/bin/env python3
"""Build the public skills-only plugin archive without local cache metadata."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from tempfile import NamedTemporaryFile
from zipfile import ZIP_DEFLATED, ZipFile


PLUGIN_ROOT = Path(__file__).resolve().parents[1]
INCLUDED_ROOT_FILES = ("plugin.json",)
INCLUDED_DIRECTORIES = (".codex-plugin", "skills", "assets")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=PLUGIN_ROOT / "dist" / "socai-social-research-skills-only.zip",
    )
    return parser.parse_args()


def public_version() -> str:
    manifest = json.loads((PLUGIN_ROOT / "plugin.json").read_text(encoding="utf-8"))
    version = manifest.get("version")
    if not isinstance(version, str) or not version:
        raise ValueError("plugin.json must contain a non-empty version")
    return version


def files_to_package() -> list[Path]:
    files = [PLUGIN_ROOT / name for name in INCLUDED_ROOT_FILES]
    for directory in INCLUDED_DIRECTORIES:
        files.extend(path for path in (PLUGIN_ROOT / directory).rglob("*") if path.is_file())
    return sorted(files, key=lambda path: path.relative_to(PLUGIN_ROOT).as_posix())


def file_bytes(path: Path, version: str) -> bytes:
    relative = path.relative_to(PLUGIN_ROOT).as_posix()
    if relative == ".codex-plugin/plugin.json":
        manifest = json.loads(path.read_text(encoding="utf-8"))
        manifest["version"] = version
        return (json.dumps(manifest, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    return path.read_bytes()


def validate_output(output: Path) -> None:
    try:
        relative = output.relative_to(PLUGIN_ROOT)
    except ValueError:
        return

    if relative.as_posix() in INCLUDED_ROOT_FILES or (
        relative.parts and relative.parts[0] in INCLUDED_DIRECTORIES
    ):
        raise ValueError(f"output must not replace or sit inside packaged input: {output}")


def main() -> None:
    args = parse_args()
    output = args.output.expanduser().resolve()
    validate_output(output)
    files = files_to_package()
    for path in files:
        if path.is_symlink():
            raise ValueError(f"refusing to package symbolic link: {path}")

    output.parent.mkdir(parents=True, exist_ok=True)
    version = public_version()
    with NamedTemporaryFile(
        prefix=f".{output.name}.", suffix=".tmp", dir=output.parent, delete=False
    ) as temporary:
        temporary_path = Path(temporary.name)

    try:
        with ZipFile(temporary_path, "w", ZIP_DEFLATED) as archive:
            for path in files:
                archive.writestr(
                    path.relative_to(PLUGIN_ROOT).as_posix(),
                    file_bytes(path, version),
                )
        temporary_path.replace(output)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise
    print(output)


if __name__ == "__main__":
    main()
