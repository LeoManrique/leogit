"""The product version, and the four files that have to agree about it.

LeoGit ships one version across two clients: a release is one tag, and the
artifact each platform gets is a build of whichever client covers it. So the
version is a property of the *product*, not of either frontend, and every file
that states it has to be moved together.

`tauri.conf.json` is the declared source of truth — it is the file the running
Tauri build's own version is read back from — and the rest are kept in step
with it:

| File                                      | Setting                                    |
|-------------------------------------------|--------------------------------------------|
| `apps/tauri-app/src-tauri/tauri.conf.json`| `"version"` — the source of truth          |
| `apps/tauri-app/package.json`             | `"version"`                                |
| `apps/tauri-app/src-tauri/Cargo.toml`     | `version` — what `check_for_update` reads  |
| `apps/swift-ui-app/project.yml`           | `MARKETING_VERSION`, `CURRENT_PROJECT_VERSION` |
| `Cargo.lock`                              | resynced by `cargo update -w`              |

Two of those are load-bearing beyond bookkeeping. The Tauri host passes its
crate's `CARGO_PKG_VERSION` to `core::update::check_for_update`, so a stale
`Cargo.toml` makes that build announce an update to itself; and XcodeGen writes
`MARKETING_VERSION` into the generated `Info.plist` as
`CFBundleShortVersionString`, which is where the native app reads the same
answer. `leogit-core` and `leogit-ffi` deliberately keep their own `0.1.0`
manifest versions: they are libraries, no release is named after them, and
nothing compares against them any more.

Not a script: it does nothing on its own.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path

from _common import MAC_DIR, REPO_ROOT, TAURI_DIR, capture, error, success, warn

TAURI_CONF = TAURI_DIR / "src-tauri" / "tauri.conf.json"
PACKAGE_JSON = TAURI_DIR / "package.json"
CARGO_TOML = TAURI_DIR / "src-tauri" / "Cargo.toml"
PROJECT_YML = MAC_DIR / "project.yml"
CARGO_LOCK = REPO_ROOT / "Cargo.lock"

VERSION_PATTERN = re.compile(r"\d+\.\d+\.\d+")


@dataclass(frozen=True)
class Setting:
    """One version-bearing line, and how to find it.

    Every pattern is anchored to the whole line and split into three groups —
    lead, value, tail — so a rewrite can only touch the value and keeps the
    original indentation and trailing comma. Matching loosely here is how a
    substitution ends up in a dependency's version instead of the package's.
    """

    path: Path
    pattern: re.Pattern[str]
    what: str


# `"version"` is the second key of both JSON files and appears nowhere else in
# either: package.json's dependencies are `"name": "^1.2.3"` pairs, and
# tauri.conf.json has one. Anchoring to two-space indentation keeps it that way
# even if a nested `"version"` is ever added.
_JSON_VERSION = r'^(?P<lead>  "version"\s*:\s*")(?P<value>[0-9][0-9.]*)(?P<tail>",?\s*)$'

SETTINGS = (
    Setting(TAURI_CONF, re.compile(_JSON_VERSION, re.MULTILINE), "tauri.conf.json version"),
    Setting(PACKAGE_JSON, re.compile(_JSON_VERSION, re.MULTILINE), "package.json version"),
    Setting(
        CARGO_TOML,
        # Anchored to the start of the line, which only the `[package]` version
        # is: a dependency states its version as `name = "x"` or
        # `name = { version = … }`, never in the first column.
        re.compile(r'^(?P<lead>version = ")(?P<value>[0-9][0-9.]*)(?P<tail>"\s*)$', re.MULTILINE),
        "Cargo.toml package version",
    ),
    Setting(
        PROJECT_YML,
        re.compile(
            r'^(?P<lead>\s*MARKETING_VERSION:\s*")(?P<value>[0-9][0-9.]*)(?P<tail>"\s*)$',
            re.MULTILINE,
        ),
        "project.yml MARKETING_VERSION",
    ),
)

_PROJECT_BUILD_NUMBER = re.compile(
    r'^(?P<lead>\s*CURRENT_PROJECT_VERSION:\s*")(?P<value>[0-9]+)(?P<tail>"\s*)$',
    re.MULTILINE,
)


def _read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        error(f"Could not read {path}: {exc}")


def _write(path: Path, text: str) -> None:
    try:
        path.write_text(text, encoding="utf-8")
    except OSError as exc:
        error(f"Could not write {path}: {exc}")


def _value(setting: Setting) -> str:
    match = setting.pattern.search(_read(setting.path))
    if match is None:
        error(f"Could not read the {setting.what} from {setting.path}")
    return match.group("value")


def read_version() -> str:
    """The product version, from the source of truth."""
    return _value(SETTINGS[0])


def out_of_step() -> list[str]:
    """Files whose version disagrees with `tauri.conf.json`.

    A release refuses to run against a tree in this state rather than bumping
    past it: a file left behind means the last release did not finish, and
    quietly overwriting it would hide that.
    """
    wanted = read_version()
    return [s.what for s in SETTINGS[1:] if _value(s) != wanted]


def set_version(version: str) -> None:
    """Write `version` into all four files and step the macOS build number.

    The build number moves too because it answers a different question.
    `MARKETING_VERSION` is `CFBundleShortVersionString`, the version a person
    reads; `CURRENT_PROJECT_VERSION` is `CFBundleVersion`, the number Apple
    requires to increase between submissions and the one an updater like
    Sparkle compares. Leaving it at 1 for ever costs nothing today and
    forecloses that later.
    """
    for setting in SETTINGS:
        text = _read(setting.path)
        # `re.sub` reports "no match" by returning the subject unchanged —
        # exactly the silent no-op that makes `sed` the wrong tool here — so
        # count the substitutions and insist on precisely one.
        text, hits = setting.pattern.subn(
            lambda m: f"{m['lead']}{version}{m['tail']}", text
        )
        if hits != 1:
            error(f"Expected one {setting.what} in {setting.path}, matched {hits}")
        _write(setting.path, text)

    text = _read(PROJECT_YML)
    match = _PROJECT_BUILD_NUMBER.search(text)
    if match is None:
        error(f"Could not read CURRENT_PROJECT_VERSION from {PROJECT_YML}")
    build = int(match.group("value")) + 1
    text, hits = _PROJECT_BUILD_NUMBER.subn(lambda m: f"{m['lead']}{build}{m['tail']}", text)
    if hits != 1:
        error(f"Expected one CURRENT_PROJECT_VERSION in {PROJECT_YML}, matched {hits}")
    _write(PROJECT_YML, text)

    # Cargo.lock records the workspace member's own version. Resync it now
    # (`-w` touches workspace members only, never a dependency pin) so the
    # release build does not do it and leave the tree dirty behind the tag.
    code, _ = capture(["cargo", "update", "-w"], cwd=REPO_ROOT)
    if code != 0:
        warn("cargo update -w failed; Cargo.lock may be left stale")

    if read_version() != version:
        error(f"Failed to set the version (still {read_version()})")
    if stale := out_of_step():
        error(f"Version bump did not reach: {', '.join(stale)}")


def changed_version_files() -> list[str]:
    """The bumped files, as repo-relative paths for `git add`."""
    return [
        str(path.relative_to(REPO_ROOT))
        for path in (TAURI_CONF, PACKAGE_JSON, CARGO_TOML, PROJECT_YML, CARGO_LOCK)
    ]


def version_tuple(version: str) -> tuple[int, ...]:
    """Comparable form of an `x.y.z` version.

    Numeric rather than lexicographic, which is the ordering the app itself
    applies to the same tags (`core::update::parse3`) — a string compare puts
    0.1.30 below 0.1.4.
    """
    parts = [int(part) for part in version.split(".")]
    return tuple((parts + [0, 0, 0])[:3])


def require_valid(version: str) -> None:
    if not VERSION_PATTERN.fullmatch(version):
        error(f"Version must be x.y.z (got: {version})")


def report() -> None:
    """Print the current version, and say so if any file has drifted."""
    version = read_version()
    if stale := out_of_step():
        error(f"Version files disagree with tauri.conf.json ({version}): {', '.join(stale)}")
    success(f"Version: {version}")
