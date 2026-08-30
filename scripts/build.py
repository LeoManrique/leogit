#!/usr/bin/env python3
"""Builds the LeoGit bundle this platform releases.

    python3 scripts/build.py                  # the client this platform ships
    python3 scripts/build.py --client tauri   # the Tauri client, wherever it builds

Outputs, by platform:

    macOS     apps/swift-ui-app/build/Build/Products/Release/LeoGit.app
    Linux     target/release/bundle/appimage/*.AppImage
    Windows   target/release/bundle/nsis/*-setup.exe

macOS ships the native SwiftUI client; Linux and Windows ship Tauri, which is
the only client that builds for them. `--client tauri` on macOS builds the Tauri
bundle anyway — it is still a supported client there, just not the one a release
publishes — and is how that build stays exercised. There is no `--client native`
override: the SwiftUI app builds on nothing else.

The version is read from the version files, never passed in. To release a new
one use deploy_release.py, which bumps and commits before it builds.
"""

from __future__ import annotations

import argparse

from _build import build, require_toolchain
from _common import Client, RELEASE_CLIENT, Steps, error, finish, host_os, success
from _version import read_version, report

step = Steps(3)


def main() -> None:
    parser = argparse.ArgumentParser(description="Build the LeoGit bundle for this platform.")
    parser.add_argument(
        "--client",
        choices=[Client.TAURI],
        type=Client,
        help="build the Tauri client instead of the one this platform releases",
    )
    args = parser.parse_args()

    target_os = host_os()
    client = args.client or RELEASE_CLIENT[target_os]
    if client is Client.NATIVE and target_os != "macOS":
        error(f"The native client is macOS-only (this machine is {target_os})")

    # ── Step 1: Validate prerequisites ──
    step("Validating prerequisites")
    success(f"Platform: {target_os}, client: {client}")
    require_toolchain(client)

    # ── Step 2: Determine version ──
    step("Determining version")
    report()

    # ── Step 3: Build ──
    step(f"Building LeoGit {read_version()} (this takes a while)")
    built = build(client)

    finish(f"Built LeoGit {read_version()}", str(built))


if __name__ == "__main__":
    main()
