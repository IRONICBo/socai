#!/usr/bin/env python3
"""Build host-specific archives from the portable Socai Agent Skill."""

from __future__ import annotations

import argparse
import stat
from pathlib import Path, PurePosixPath
from tempfile import NamedTemporaryFile
from zipfile import ZIP_DEFLATED, ZipFile


PLUGIN_ROOT = Path(__file__).resolve().parents[1]
DIST = PLUGIN_ROOT / "dist"
SKILL_ROOT = PLUGIN_ROOT / "skills" / "socai-social-research"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "target",
        nargs="?",
        default="all",
        choices=("all", "codebuddy", "gemini", "kimi", "trae"),
    )
    return parser.parse_args()


def collect_tree(root: Path, prefix: str) -> list[tuple[Path, str]]:
    validate_source(root, require_file=False)
    files: list[tuple[Path, str]] = []
    for path in root.rglob("*"):
        validate_source(path, require_file=False)
        if path.is_file():
            relative = path.relative_to(root).as_posix()
            files.append((path, f"{prefix}{relative}"))
    return files


def validate_source(path: Path, *, require_file: bool = True) -> None:
    """Reject links anywhere below the plugin root before reading package input."""
    try:
        relative = path.relative_to(PLUGIN_ROOT)
    except ValueError as error:
        raise ValueError(f"package input is outside the plugin root: {path}") from error

    current = PLUGIN_ROOT
    if stat.S_ISLNK(current.lstat().st_mode):
        raise ValueError(f"refusing to package symbolic link: {current}")
    for part in relative.parts:
        current = current / part
        try:
            metadata = current.lstat()
        except FileNotFoundError as error:
            raise ValueError(f"package input does not exist: {current}") from error
        if stat.S_ISLNK(metadata.st_mode):
            raise ValueError(f"refusing to package symbolic link: {current}")

    if require_file and not path.is_file():
        raise ValueError(f"package input is not a regular file: {path}")


def package(name: str, files: list[tuple[Path, str]]) -> Path:
    output = DIST / f"socai-social-research-{name}.zip"
    output.parent.mkdir(parents=True, exist_ok=True)
    with NamedTemporaryFile(
        prefix=f".{output.name}.", suffix=".tmp", dir=output.parent, delete=False
    ) as temporary:
        temporary_path = Path(temporary.name)
    try:
        with ZipFile(temporary_path, "w", ZIP_DEFLATED) as archive:
            seen: set[str] = set()
            for path, archive_name in sorted(files, key=lambda item: item[1]):
                validate_source(path)
                parts = PurePosixPath(archive_name).parts
                if (
                    archive_name.startswith("/")
                    or ".." in parts
                    or "\\" in archive_name
                    or archive_name in seen
                ):
                    raise ValueError(f"unsafe or duplicate archive entry: {archive_name}")
                seen.add(archive_name)
                archive.write(path, archive_name)
        validate_archive(temporary_path)
        temporary_path.replace(output)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise
    print(output)
    return output


def validate_archive(path: Path) -> None:
    with ZipFile(path) as archive:
        for entry in archive.infolist():
            parts = PurePosixPath(entry.filename).parts
            mode = entry.external_attr >> 16
            if (
                entry.filename.startswith("/")
                or ".." in parts
                or "\\" in entry.filename
                or stat.S_ISLNK(mode)
            ):
                raise ValueError(f"unsafe archive entry: {entry.filename}")
        corrupt = archive.testzip()
        if corrupt:
            raise ValueError(f"corrupt archive entry: {corrupt}")


def common_skill_files(prefix: str = "skills/socai-social-research/") -> list[tuple[Path, str]]:
    return collect_tree(SKILL_ROOT, prefix)


def codebuddy_files() -> list[tuple[Path, str]]:
    return collect_tree(PLUGIN_ROOT / ".codebuddy-plugin", ".codebuddy-plugin/") + common_skill_files()


def gemini_files() -> list[tuple[Path, str]]:
    return [(PLUGIN_ROOT / "gemini-extension.json", "gemini-extension.json")] + common_skill_files()


def kimi_files() -> list[tuple[Path, str]]:
    return [(PLUGIN_ROOT / "kimi.plugin.json", "kimi.plugin.json")] + common_skill_files()


def trae_files() -> list[tuple[Path, str]]:
    return collect_tree(SKILL_ROOT, "")


def main() -> None:
    target = parse_args().target
    builders = {
        "codebuddy": codebuddy_files,
        "gemini": gemini_files,
        "kimi": kimi_files,
        "trae": trae_files,
    }
    selected = builders.items() if target == "all" else [(target, builders[target])]
    for name, builder in selected:
        package(name, builder())


if __name__ == "__main__":
    main()
