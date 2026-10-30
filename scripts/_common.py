"""Facts and plumbing every release script agrees on.

Console output, subprocess handling, where things live, and which client ships
on which platform. `_version.py` and `_build.py` build on this; the scripts
build on all three.

Not a script: it does nothing on its own.

## What is *not* here

`install.sh`. It is the one script that runs on a machine with no checkout —
it is fetched and piped — so it cannot import anything and duplicates the
platform detection and artifact naming below, plus the Linux launcher script
that `_build.py` writes. Each copy must be changed with its twin; every file
says so where its half of the duplication lives.
"""

from __future__ import annotations

import os
import re
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
#
# UTF-8 on both streams for the same reason: on Windows a redirected stream
# falls back to the ANSI code page (cp1252), which has no ✓/✗/⚠, so the first
# status line raised UnicodeEncodeError. A no-op where UTF-8 is already used.
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(line_buffering=True, encoding="utf-8")
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

# Where a Linux install puts its four pieces. Spelled out under `$HOME` rather
# than through `XDG_DATA_HOME` because `install.sh` writes exactly these paths,
# and a machine that has run both installers must not end up with two copies in
# two places — whichever ran last has to land on top of the other.
LOCAL_BIN = Path.home() / ".local" / "bin"
APPIMAGE_DEST = LOCAL_BIN / "leogit.AppImage"
LAUNCHER_DEST = LOCAL_BIN / "leogit"
ICON_DEST = Path.home() / ".local" / "share" / "icons" / "leogit.png"
DESKTOP_DEST = Path.home() / ".local" / "share" / "applications" / "leogit.desktop"

# The uninstall entry Tauri's NSIS installer writes, keyed by `productName`.
# A per-user install, so it lives under HKCU and needs no elevation to read.
WINDOWS_UNINSTALL_KEY = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\leogit"
WINDOWS_EXE_NAME = "leogit.exe"


def windows_install_dir() -> Path:
    """Where LeoGit is installed on this Windows machine.

    Read from the installer's own uninstall entry rather than assumed: NSIS
    reuses the folder a previous install chose, so the per-user default
    (`%LOCALAPPDATA%\\leogit`) is only the answer when nothing is installed yet.
    """
    import winreg  # noqa: PLC0415 — Windows-only module

    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, WINDOWS_UNINSTALL_KEY) as key:
            location, _ = winreg.QueryValueEx(key, "InstallLocation")
        # Stored quoted, the way an uninstall entry's paths conventionally are.
        return Path(str(location).strip('"'))
    except OSError:
        return Path(os.environ["LOCALAPPDATA"]) / "leogit"


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
def _resolved(command: list[str]) -> list[str] | None:
    """`command` with its executable replaced by its full path, or None if absent.

    Every subprocess goes through here so a tool is looked up exactly the way
    `require_tools` looked it up. Handing the bare name to `subprocess.run`
    is not the same lookup on Windows: it becomes a CreateProcess call, which
    tries only `<name>` and `<name>.exe`, while npm installs its tools — pnpm
    among them — as `<name>.cmd` shims that only a PATHEXT-aware search finds.
    """
    executable = shutil.which(command[0])
    if executable is None:
        return None
    return [executable, *command[1:]]


def run(command: list[str], *, cwd: Path | None = None, what: str, env: dict[str, str] | None = None) -> None:
    """Run a subcommand, aborting the script if it is missing or exits non-zero."""
    resolved = _resolved(command)
    if resolved is None:
        error(f"{what} failed: {command[0]} is not installed")
    merged = {**os.environ, **env} if env else None
    result = subprocess.run(resolved, cwd=cwd, env=merged, check=False)
    if result.returncode != 0:
        error(f"{what} failed (exit {result.returncode})")


def capture(command: list[str], *, cwd: Path | None = None) -> tuple[int, str]:
    """Run a subcommand quietly. Returns (exit code, stdout stripped); 127 if missing."""
    resolved = _resolved(command)
    if resolved is None:
        return 127, ""
    result = subprocess.run(resolved, cwd=cwd, capture_output=True, text=True, check=False)
    return result.returncode, result.stdout.strip()


def quietly(command: list[str]) -> None:
    """Run a subcommand whose failure — or absence — is not worth stopping for."""
    resolved = _resolved(command)
    if resolved is not None:
        subprocess.run(resolved, capture_output=True, check=False)


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
def _running_app_patterns() -> list[str]:
    """`pgrep -f` patterns that match a running LeoGit and nothing else.

    Matched against the whole command line, so each is anchored or carries
    enough path to be unmistakable — a bare `leogit` would also match the very
    script doing the matching.
    """
    if host_os() == "macOS":
        # The path inside the bundle rather than the bare executable name: on a
        # case-insensitive volume `pgrep -x leogit` answers for both bundles.
        # Both clients are matched, not just the one being installed — they
        # occupy the same `/Applications` slot, so installing one is also
        # removing the other, and the other is exactly the copy still running.
        return [f"/Contents/MacOS/{name}" for name in ("LeoGit", "leogit")]
    # An AppImage shows up as two processes: the runtime, whose command line is
    # the image's own path, and the app it mounted and exec'd, which lives under
    # the mount point. Killing only the first would leave the second holding the
    # squashfs open.
    return [f"^{re.escape(str(APPIMAGE_DEST))}", r"^/tmp/\.mount_leogit"]


def _running_instances() -> list[str]:
    """What is running now, in the form [`_stop_instances`] takes.

    `pgrep -f` patterns that still match on macOS and Linux. On Windows, the
    PIDs of the *installed* executable — matched by path, not image name, so a
    `tauri dev` build (also `leogit.exe`) is left alone.
    """
    if host_os() == "windows":
        exe = str(windows_install_dir() / WINDOWS_EXE_NAME).replace("'", "''")
        _, pids = capture(
            [
                "powershell",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                f"Get-Process -Name leogit -ErrorAction SilentlyContinue"
                f" | Where-Object {{ $_.Path -eq '{exe}' }}"
                f" | ForEach-Object {{ $_.Id }}",
            ]
        )
        return pids.split()
    return [
        pattern
        for pattern in _running_app_patterns()
        if subprocess.run(["pgrep", "-f", pattern], capture_output=True, check=False).returncode
        == 0
    ]


def _stop_instances(instances: list[str], *, force: bool) -> None:
    """Ask each instance to quit, or with `force`, kill it outright.

    `taskkill` without `/F` posts WM_CLOSE to the app's windows, the same as
    clicking the close button — Windows' counterpart to SIGTERM.
    """
    for instance in instances:
        if host_os() == "windows":
            quietly(["taskkill", *(["/F"] if force else []), "/PID", instance])
        else:
            quietly(["pkill", "-KILL" if force else "-TERM", "-f", instance])


def stop_running_app() -> None:
    """Stop any running copy before an install overwrites what it is running.

    A running app holds its executable open. On macOS, overwriting the bundle
    around it leaves a half-old install until the next launch; on Linux it is
    worse, because the AppImage file *is* the mounted filesystem the running app
    is still reading pages from; on Windows the installer cannot replace an
    executable that is running at all.
    """
    if not _running_instances():
        success("No running instances")
        return

    _stop_instances(_running_instances(), force=False)
    for _ in range(16):
        if not _running_instances():
            break
        time.sleep(0.5)
    if stubborn := _running_instances():
        warn(f"Force-killing {len(stubborn)} stubborn instance(s) (graceful stop timed out)")
        _stop_instances(stubborn, force=True)
    success("Stopped the running app")
