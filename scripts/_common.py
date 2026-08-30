"""Facts and plumbing every release script agrees on.

Console output, subprocess handling, where things live, and which client ships
on which platform. `_version.py` and `_build.py` build on this; the scripts
build on all three.

Not a script: it does nothing on its own.

## What is *not* here

`install.sh`. It is the one script that runs on a machine with no checkout —
it is fetched and piped — so it cannot import anything and duplicates the
platform detection and artifact naming below. The two must be changed
together; each file says so where the duplication lives.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import time
from enum import StrEnum
from pathlib import Path
from typing import NoReturn

# ── Console ──
# These scripts print around subprocesses that write straight to the same
# stream. Python block-buffers its own output as soon as that stream is a pipe,
# so without this a redirected run comes back with every step header collected
# at the end, after the build log they were meant to introduce.
try:
    sys.stdout.reconfigure(line_buffering=True)
except (AttributeError, OSError):
    pass

# Windows terminals need ANSI enabling; macOS and Linux interpret the escapes
# natively. `colorama`-free: `os.system("")` is enough to flip the console mode
# on modern Windows, and it is a no-op elsewhere.
if sys.platform == "win32":
    os.system("")  # noqa: S605 — flips ENABLE_VIRTUAL_TERMINAL_PROCESSING

_COLOR = not os.environ.get("NO_COLOR") and sys.stdout.isatty()
RED, GREEN, YELLOW, BLUE, CYAN, NC = (
    ("\033[0;31m", "\033[0;32m", "\033[1;33m", "\033[0;34m", "\033[0;36m", "\033[0m")
    if _COLOR
    else ("", "", "", "", "", "")
)


class Steps:
    """Numbered progress: `step = Steps(4)` then `step("Doing the thing")`.

    The number comes from the call order rather than the caller, so inserting a
    step cannot leave the script counting "[2/4]" twice.
    """

    def __init__(self, total: int) -> None:
        self.total = total
        self.done = 0

    def __call__(self, message: str) -> None:
        self.done += 1
        print(f"\n{BLUE}[{self.done}/{self.total}]{NC} {CYAN}{message}{NC}")


def info(message: str) -> None:
    print(f"{CYAN}{message}{NC}")


def success(message: str) -> None:
    print(f"  {GREEN}✓ {message}{NC}")


def warn(message: str) -> None:
    print(f"  {YELLOW}⚠ {message}{NC}")


def error(message: str) -> NoReturn:
    print(f"  {RED}✗ {message}{NC}", file=sys.stderr)
    sys.exit(1)


def finish(headline: str, *hints: str) -> None:
    print(f"\n{GREEN}═══ {headline} ═══{NC}")
    for hint in hints:
        print(f"  {CYAN}{hint}{NC}")


# ── Identity and paths ──
REPO = "LeoManrique/leogit"

REPO_ROOT = Path(__file__).resolve().parent.parent
TAURI_DIR = REPO_ROOT / "apps" / "tauri-app"
MAC_DIR = REPO_ROOT / "apps" / "swift-ui-app"
# The Cargo workspace relocates target/ to the repo root, so both the Tauri
# bundles and the static library the macOS app links land here rather than
# under either app.
TARGET_DIR = REPO_ROOT / "target"
DIST_DIR = REPO_ROOT / "dist"

APPLICATIONS = Path("/Applications")
LSREGISTER = Path(
    "/System/Library/Frameworks/CoreServices.framework/Frameworks"
    "/LaunchServices.framework/Support/lsregister"
)


class Client(StrEnum):
    """Which of the two frontends a build is of.

    They are one product, not two: one version, one release, one artifact per
    platform. Which client fills a platform's slot is [`RELEASE_CLIENT`].
    """

    NATIVE = "native"
    TAURI = "tauri"


# The bundle each client produces. On macOS these two names are the *same path*
# — the boot volume is case-insensitive by default — which is half the reason
# only one client ships per platform.
BUNDLE_NAME = {Client.NATIVE: "LeoGit.app", Client.TAURI: "leogit.app"}

# The client a release is built from, per platform. macOS ships the native
# SwiftUI app; Linux and Windows ship Tauri, which is the only client that
# builds for them.
#
# One per platform is a requirement, not a preference: `core::update`'s
# `artifact_name` decides whether a release contains *this* build's artifact
# using `cfg!(target_os)` alone, and no cfg distinguishes a native macOS build
# from a Tauri one. Two macOS artifacts in a release would leave each client
# looking for a name that could belong to either.
RELEASE_CLIENT = {
    "macOS": Client.NATIVE,
    "linux": Client.TAURI,
    "windows": Client.TAURI,
}


# ── Platform ──
def host_os() -> str:
    """This machine as the release artifacts name it.

    Mirrors `core::update::artifact_name`'s `cfg!(target_os)` arms and
    `install.sh`'s `uname -s` case, which is why the macOS spelling keeps its
    capital S.
    """
    if sys.platform == "darwin":
        return "macOS"
    if sys.platform.startswith("linux"):
        return "linux"
    if sys.platform in ("win32", "cygwin", "msys"):
        return "windows"
    error(f"Unsupported OS: {sys.platform} (LeoGit builds on macOS, Linux and Windows)")


def host_arch() -> str:
    """`arm64` or `amd64`, named the way the artifacts are.

    On Windows the answer comes from rustc rather than from Python: an x86_64
    MSYS2 shell emulated on an ARM64 host still builds — and NSIS-packages —
    `aarch64-pc-windows-msvc`, so the shell's own idea of the architecture
    describes the wrong thing. Elsewhere the interpreter's architecture is the
    build's.
    """
    if host_os() == "windows":
        code, host = capture(["rustc", "-vV"])
        triple = next(
            (line.removeprefix("host: ") for line in host.splitlines() if line.startswith("host: ")),
            "",
        )
        if code != 0 or not triple:
            error("Could not read rustc's host triple to name the Windows artifact")
        if triple.startswith("aarch64-"):
            return "arm64"
        if triple.startswith("x86_64-"):
            return "amd64"
        error(f"Unsupported Windows build target: {triple}")

    machine = os.uname().machine
    if machine in ("arm64", "aarch64"):
        return "arm64"
    if machine == "x86_64":
        return "amd64"
    error(f"Unsupported architecture: {machine}")


def artifact_name(version: str, *, target_os: str | None = None, arch: str | None = None) -> str:
    """The release asset for `version` on a platform.

    Byte-identical to `core::update::artifact_name`, which is what the running
    app uses to decide whether a newer release actually contains its own
    artifact — a release is published one platform at a time, so a name that
    drifts from core's does not fail loudly, it silently hides every update.
    A golden test in `core/src/update.rs` pins the same strings from the other
    side; `install.sh` reconstructs them a third time.
    """
    target_os = target_os or host_os()
    arch = arch or host_arch()
    if target_os == "windows":
        # NSIS installers are conventionally suffixed "-setup.exe".
        return f"LeoGit-{version}-windows-{arch}-setup.exe"
    if target_os == "macOS":
        return f"LeoGit-{version}-macOS-{arch}.zip"
    return f"LeoGit-{version}-linux-{arch}.AppImage"


# ── Subprocesses ──
def run(command: list[str], *, cwd: Path | None = None, what: str, env: dict[str, str] | None = None) -> None:
    """Run a subcommand, aborting the script if it exits non-zero."""
    merged = {**os.environ, **env} if env else None
    result = subprocess.run(command, cwd=cwd, env=merged, check=False)
    if result.returncode != 0:
        error(f"{what} failed (exit {result.returncode})")


def capture(command: list[str], *, cwd: Path | None = None) -> tuple[int, str]:
    """Run a subcommand quietly. Returns (exit code, stdout stripped)."""
    try:
        result = subprocess.run(command, cwd=cwd, capture_output=True, text=True, check=False)
    except FileNotFoundError:
        return 127, ""
    return result.returncode, result.stdout.strip()


def quietly(command: list[str]) -> None:
    """Run a subcommand whose failure is not worth stopping for."""
    subprocess.run(command, capture_output=True, check=False)


def require_tools(*names: str) -> None:
    for name in names:
        if shutil.which(name) is None:
            error(f"{name} is not installed")
        success(f"{name} found")


def require_gh_auth() -> None:
    code, _ = capture(["gh", "auth", "status"])
    if code != 0:
        error("gh is not authenticated. Run: gh auth login")
    success("gh authenticated")


def require_macos() -> None:
    if host_os() != "macOS":
        error("This script only runs on macOS")


# ── Git ──
def git(*args: str, what: str) -> None:
    run(["git", *args], cwd=REPO_ROOT, what=what)


def working_tree_is_dirty() -> bool:
    """Any staged or unstaged change anywhere in the tree.

    Checked before a release because a tag is a claim about what was built, and
    a dirty tree makes that claim unverifiable.
    """
    unstaged, _ = capture(["git", "diff", "--quiet"], cwd=REPO_ROOT)
    staged, _ = capture(["git", "diff", "--cached", "--quiet"], cwd=REPO_ROOT)
    return unstaged != 0 or staged != 0


# ── Running instances ──
def stop_running_app() -> None:
    """Stop any running copy of either client before replacing a bundle.

    A running app holds its executable open; overwriting the bundle around it
    leaves a half-old install until the next launch. Both clients are matched,
    not just the one being installed: on macOS they occupy the same
    `/Applications` slot, so installing one is also removing the other, and the
    other is exactly the copy that would still be running.
    """
    executables = ("LeoGit", "leogit")

    def running() -> list[str]:
        return [
            name
            for name in executables
            # Match the path inside the bundle rather than the bare process
            # name: on a case-insensitive volume `pgrep -x leogit` answers for
            # both bundles, and `pgrep -f leogit` would also match this very
            # script's command line.
            if subprocess.run(
                ["pgrep", "-f", f"/Contents/MacOS/{name}"], capture_output=True, check=False
            ).returncode
            == 0
        ]

    if not running():
        success("No running instances")
        return

    for name in running():
        quietly(["pkill", "-TERM", "-f", f"/Contents/MacOS/{name}"])
    for _ in range(16):
        if not running():
            break
        time.sleep(0.5)
    if stubborn := running():
        warn(f"Force-killing {', '.join(stubborn)} (graceful stop timed out)")
        for name in stubborn:
            quietly(["pkill", "-KILL", "-f", f"/Contents/MacOS/{name}"])
    success("Stopped the running app")
