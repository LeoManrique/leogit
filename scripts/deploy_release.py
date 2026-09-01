#!/usr/bin/env python3
"""Builds LeoGit and publishes it as a GitHub release.

    python3 scripts/deploy_release.py          # release the current version
    python3 scripts/deploy_release.py 1.1.0    # bump to 1.1.0 first, then release

## One release, three platforms, run once each

A release holds one artifact per platform, and each is built on a machine of
that kind — there is no cross-compilation here. So this script runs once on
macOS, once on Linux and once on Windows, against the same tag: the first run
creates the release, the rest upload into it. `core::update` knows that and
withholds an update whose artifact for the running platform has not landed yet,
so a half-published release is quiet rather than broken.

Which client fills a platform's slot is `_common.RELEASE_CLIENT`: macOS ships
the native SwiftUI app, Linux and Windows ship Tauri.

## How the version is updated

`_version.py` owns the list of files that state it and moves them together.
Given a version argument this script bumps them, commits as "Bump version to
x.y.z" and pushes before tagging anything; with no argument it releases whatever
the tree already holds and leaves the files alone.

Re-running a version that has already been released is the supported way to
retry: the tag is reused, and the artifact replaces the one on the release or
creates the release if there is none. A run that died at the upload is fixed by
running it again, with no version invented to get past it.
"""

from __future__ import annotations

import argparse
from pathlib import Path

from _build import build, package, require_toolchain
from _common import (
    REPO,
    RELEASE_CLIENT,
    Steps,
    capture,
    error,
    finish,
    git,
    host_os,
    require_gh_auth,
    require_tools,
    run,
    success,
    warn,
    working_tree_is_dirty,
)
from _version import (
    changed_version_files,
    out_of_step,
    read_version,
    require_valid,
    set_version,
    version_tuple,
)

step = Steps(6)


def resolve_version(requested: str | None) -> str:
    """The version to release, bumping and committing the version files first
    if a new one was asked for."""
    current = read_version()

    if requested is None:
        success(f"Releasing the current version: {current}")
        return current

    require_valid(requested)
    if requested == current:
        success(f"Version {current} is already current, no bump needed")
        return current
    if version_tuple(requested) < version_tuple(current):
        error(f"Version {requested} is older than the current {current}")

    set_version(requested)
    git("add", *changed_version_files(), what="git add")
    git("commit", "-m", f"Bump version to {requested}", what="git commit")
    git("push", what="git push")
    success(f"Bumped {current} → {requested} and pushed")
    return requested


def refuse_to_ship_behind_latest(version: str) -> None:
    """Never publish onto a tag older than GitHub's own `latest`.

    `install.sh` installs from `/releases/latest`, so uploading artifacts to an
    older tag leaves them invisible to installers *and* can strand the real
    latest release without this platform's build. A version this stale almost
    always means the local tree is behind origin, and the fix is `git pull`
    rather than a version invented to get past the check.
    """
    found, tag = capture(["gh", "api", f"repos/{REPO}/releases/latest", "--jq", ".tag_name"])
    if found != 0 or not tag:
        success("No published release yet — this will be the first")
        return
    latest = tag.removeprefix("v")
    if version != latest and version_tuple(version) < version_tuple(latest):
        error(
            f"Refusing to release v{version}: GitHub's latest release is already {tag}. "
            "Your local tree is likely behind origin — run 'git pull' (then bump if you "
            "intend a new version) and retry."
        )
    success(f"Latest GitHub release: {tag}")


def tag_release(tag: str) -> None:
    exists, _ = capture(["git", "rev-parse", tag])
    if exists == 0:
        warn(f"Tag {tag} already exists locally, reusing it")
    else:
        git("tag", "-a", tag, "-m", f"Release {tag}", what="git tag")
    # Pushed unconditionally rather than only when freshly created: pushing a
    # tag the remote already has at the same commit is a no-op, while skipping
    # it is not — a tag left behind by a run that failed later would never reach
    # the remote, and `gh release create` would then invent one at the branch
    # head instead of using the commit that was actually built.
    git("push", "origin", tag, what="git push --tags")
    success(f"Tagged {tag}")


def release_notes(version: str) -> str:
    """Notes covering every platform, whichever one publishes first.

    The other platforms upload their artifacts with --clobber and leave the
    notes alone, so these have to describe the whole release rather than the
    run that happened to create it.
    """
    return f"""## LeoGit v{version}

A fast, native Git client. macOS runs the SwiftUI client; Linux and Windows run
the Tauri one.

### Install

```
curl -fsSL https://raw.githubusercontent.com/{REPO}/main/scripts/install.sh | bash
```

The installer detects your platform (macOS and Linux) and also adds the
`leogit [dir]` shell command for opening a repository from a terminal.

**macOS** — or download `LeoGit-{version}-macOS-*.zip` below and drag `LeoGit.app` into `/Applications`. The bundle is ad-hoc signed, so on first launch right-click → Open (or run `xattr -dr com.apple.quarantine /Applications/LeoGit.app` — the install script does this for you).

**Linux** — or download `LeoGit-{version}-linux-*.AppImage` below, `chmod +x` it, and run. The install script also drops a launcher in your app menu.

**Windows** — download `LeoGit-{version}-windows-*-setup.exe` below and run it. It installs per-user (no admin), adds a Start Menu shortcut, and pulls in the WebView2 runtime if it is missing.

### Requirements

- macOS 26 or later (Apple silicon or Intel, matching the artifact's architecture)
- Linux (arm64 or amd64, matching the artifact's architecture) with FUSE 2 (Arch: `sudo pacman -S fuse2`)
- Windows 10/11 (the installer bundles the WebView2 bootstrapper)"""


def publish(tag: str, artifact: Path) -> None:
    """Upload into the release for `tag`, creating it if this is the first
    platform to finish."""
    exists, _ = capture(["gh", "release", "view", tag, "--repo", REPO])
    if exists == 0:
        warn(f"Release {tag} already exists, uploading this platform's artifact")
        command = ["gh", "release", "upload", tag, str(artifact), "--clobber", "--repo", REPO]
    else:
        command = [
            "gh",
            "release",
            "create",
            tag,
            str(artifact),
            "--repo",
            REPO,
            "--title",
            f"LeoGit {tag}",
            "--notes",
            release_notes(tag.removeprefix("v")),
        ]
    run(command, what="gh release")
    success(f"Uploaded {artifact.name} to {tag}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Build LeoGit and publish it as a GitHub release."
    )
    parser.add_argument(
        "version",
        nargs="?",
        help="new version as x.y.z; bumps and commits the version files first "
        "(default: release the version the tree already holds)",
    )
    args = parser.parse_args()

    target_os = host_os()
    client = RELEASE_CLIENT[target_os]

    # ── Step 1: Validate prerequisites ──
    step("Validating prerequisites")
    success(f"Platform: {target_os}, client: {client}")
    require_tools("git", "gh")
    require_gh_auth()
    require_toolchain(client)
    if working_tree_is_dirty():
        error("Working tree is dirty — commit or stash before releasing")
    success("Working tree clean")
    if stale := out_of_step():
        error(
            f"Version files disagree with tauri.conf.json: {', '.join(stale)}. "
            "The last release did not finish; fix the tree before starting another."
        )

    # ── Step 2: Determine version ──
    step("Determining version")
    version = resolve_version(args.version)
    refuse_to_ship_behind_latest(version)
    tag = f"v{version}"

    # ── Step 3: Tag release ──
    # Before the build rather than after it: the tag is what the release is
    # created against, and tagging first keeps a long build from being thrown
    # away by a name collision found at the end.
    step("Tagging release")
    tag_release(tag)

    # ── Step 4: Build ──
    step(f"Building LeoGit {version} (this takes a while)")
    built = build(client)

    # ── Step 5: Package artifact ──
    step("Packaging artifact")
    artifact = package(built, version)

    # ── Step 6: Upload to the GitHub release ──
    step("Uploading to GitHub")
    publish(tag, artifact)

    finish(
        f"Release {tag} published ({target_os})",
        f"https://github.com/{REPO}/releases/tag/{tag}",
    )


if __name__ == "__main__":
    main()
