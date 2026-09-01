#!/usr/bin/env python3
"""Installs LeoGit from a build made on this machine.

    python3 scripts/install_local.py              # build, then install
    python3 scripts/install_local.py --no-build   # install the existing build
    python3 scripts/install_local.py --client tauri

Unlike install.sh, which downloads a published artifact, this installs what is
in the working tree — so a change can be exercised in a real install before it
is released.

"Install" means a different thing per platform, and this script means whichever
one the platform does: the bundle goes to /Applications on macOS; on Linux the
AppImage goes to ~/.local/bin beside the `leogit` wrapper that launches it, with
an icon and a desktop entry so the app menu lists it. Both land exactly where
install.sh puts them, so whichever installer ran last is the copy you have.

Windows is not covered: its install is an NSIS installer, which is a program to
run rather than files to place.
"""

from __future__ import annotations

import argparse
from pathlib import Path

from _build import (
    NATIVE_APP,
    TAURI_BUNDLE_DIR,
    build,
    built_version,
    install_build,
    require_toolchain,
)
from _common import (
    APPIMAGE_DEST,
    APPLICATIONS,
    BUNDLE_NAME,
    RELEASE_CLIENT,
    Client,
    Steps,
    error,
    finish,
    require_os,
    stop_running_app,
    success,
)
from _version import read_version, report

step = Steps(4)


def existing_build(client: Client, target_os: str) -> Path:
    """Where `--no-build` expects to find this platform's build."""
    if client is Client.NATIVE:
        return NATIVE_APP
    if target_os == "macOS":
        return TAURI_BUNDLE_DIR / "macos" / BUNDLE_NAME[Client.TAURI]
    # Named by Tauri after the version it built, so it cannot be spelled out
    # here the way the macOS bundles can.
    images = sorted((TAURI_BUNDLE_DIR / "appimage").glob("*.AppImage"))
    if len(images) != 1:
        error(
            f"Expected exactly one *.AppImage in {TAURI_BUNDLE_DIR / 'appimage'}, "
            f"found {len(images)}. Re-run without --no-build."
        )
    return images[0]


def destination(target_os: str) -> str:
    """Where step 4 is about to write, for its own heading."""
    return str(APPLICATIONS) if target_os == "macOS" else str(APPIMAGE_DEST.parent)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Install LeoGit from a local Release build."
    )
    parser.add_argument(
        "--no-build",
        action="store_true",
        help="install the existing Release build instead of rebuilding it",
    )
    parser.add_argument(
        "--client",
        choices=[Client.TAURI],
        type=Client,
        help="build the Tauri client explicitly (already the only client on Linux)",
    )
    args = parser.parse_args()

    target_os = require_os("macOS", "linux")
    client = args.client or RELEASE_CLIENT[target_os]

    # ── Step 1: Determine version ──
    step("Determining version")
    success(f"Client: {client}")
    report()

    # ── Step 2: Build ──
    step(f"Building LeoGit {read_version()}")
    if args.no_build:
        built = existing_build(client, target_os)
        # A macOS bundle is a directory and an AppImage is a file, so this asks
        # the only question both answer.
        if not built.exists():
            error(f"No build at {built}. Re-run without --no-build.")
        success("Skipped (using the existing build)")
    else:
        require_toolchain(client)
        built = build(client)

    # ── Step 3: Stop any running instance ──
    step("Stopping running instance")
    stop_running_app()

    # ── Step 4: Install ──
    step(f"Installing to {destination(target_os)}")
    installed = install_build(built)

    version = built_version(built)
    if target_os == "macOS":
        finish(
            f"LeoGit {version} installed",
            f'Open it from /Applications, or with: open "{installed}"',
            "Terminal command: leogit [dir] — install.sh is what writes it",
        )
    else:
        finish(
            f"LeoGit {version} installed",
            "Open it from your app menu, or run: leogit",
            f"Terminal command: leogit [dir] — that is {installed}",
        )


if __name__ == "__main__":
    main()
