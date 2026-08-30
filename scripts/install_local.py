#!/usr/bin/env python3
"""Installs LeoGit to /Applications from a build made on this Mac.

    python3 scripts/install_local.py              # build, then install
    python3 scripts/install_local.py --no-build   # install the existing build
    python3 scripts/install_local.py --client tauri

Unlike install.sh, which downloads a published artifact, this installs what is
in the working tree — the Release counterpart of `just mac-run`, for keeping a
real copy in /Applications while `just mac-run` goes on launching the Debug
build out of the derived-data directory.

macOS only: it is the only platform where "install" means putting a bundle in a
known place. On Linux the AppImage is the install, and install.sh does it.
"""

from __future__ import annotations

import argparse
from pathlib import Path

from _build import (
    NATIVE_APP,
    TAURI_BUNDLE_DIR,
    build,
    bundle_version,
    install_bundle,
    require_toolchain,
)
from _common import (
    BUNDLE_NAME,
    RELEASE_CLIENT,
    Client,
    Steps,
    error,
    finish,
    require_macos,
    stop_running_app,
    success,
)
from _version import read_version, report

step = Steps(4)


def existing_build(client: Client) -> Path:
    """Where `--no-build` expects to find a bundle."""
    if client is Client.NATIVE:
        return NATIVE_APP
    return TAURI_BUNDLE_DIR / "macos" / BUNDLE_NAME[Client.TAURI]


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Install LeoGit to /Applications from a local Release build."
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
        help="install the Tauri client instead of the one this platform releases",
    )
    args = parser.parse_args()

    require_macos()
    client = args.client or RELEASE_CLIENT["macOS"]

    # ── Step 1: Determine version ──
    step("Determining version")
    success(f"Client: {client}")
    report()

    # ── Step 2: Build ──
    step(f"Building LeoGit {read_version()}")
    if args.no_build:
        bundle = existing_build(client)
        if not bundle.is_dir():
            error(f"No build at {bundle}. Re-run without --no-build.")
        success("Skipped (using the existing build)")
    else:
        require_toolchain(client)
        bundle = build(client)

    # ── Step 3: Stop any running instance ──
    step("Stopping running instance")
    stop_running_app()

    # ── Step 4: Install ──
    step("Installing to /Applications")
    installed = install_bundle(bundle)

    finish(
        f"LeoGit {bundle_version(installed)} installed",
        f'Open it from /Applications, or with: open "{installed}"',
        "Terminal command: leogit [dir] — install.sh is what writes it",
    )


if __name__ == "__main__":
    main()
