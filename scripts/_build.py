"""Building, signing, packaging and installing a LeoGit bundle.

The actions the release scripts perform, kept apart from the facts in
`_common.py` and the version handling in `_version.py`. Both clients live here
because the steps around them are the same shape — build, verify, package under
one artifact name — and only the middle differs.

Not a script: it does nothing on its own.
"""

from __future__ import annotations

import platform
import shutil
import subprocess
from pathlib import Path

from _common import (
    APPLICATIONS,
    BUNDLE_NAME,
    DIST_DIR,
    LSREGISTER,
    MAC_DIR,
    TARGET_DIR,
    TAURI_DIR,
    Client,
    artifact_name,
    capture,
    error,
    host_os,
    info,
    quietly,
    require_tools,
    run,
    success,
    warn,
)
from _version import read_version

# Where each client leaves its Release build.
NATIVE_APP = MAC_DIR / "build" / "Build" / "Products" / "Release" / BUNDLE_NAME[Client.NATIVE]
TAURI_BUNDLE_DIR = TARGET_DIR / "release" / "bundle"

# The Tauri bundle format per platform. Only the one the release ships is
# built: asking for the default set would also produce a .dmg, .deb and .msi
# nobody uploads, and on macOS the .dmg step drives Finder through AppleScript
# and fails outright without a user session.
TAURI_BUNDLE_TARGET = {"macOS": "app", "linux": "appimage", "windows": "nsis"}


# ── Prerequisites ──
def require_toolchain(client: Client) -> None:
    """Tools this client's build needs, beyond the ones every script wants."""
    if client is Client.NATIVE:
        require_tools("cargo", "xcodegen", "xcodebuild")
        return

    require_tools("pnpm", "cargo")
    if host_os() == "linux":
        # The AppImage links against WebKitGTK and is assembled by patchelf.
        # Named here with the fix rather than left to surface as a cryptic
        # linker error halfway through a long build.
        if shutil.which("patchelf") is None:
            warn("patchelf not found — AppImage bundling may fail (Arch: sudo pacman -S patchelf)")
        found, _ = capture(["pkg-config", "--exists", "webkit2gtk-4.1"])
        if found != 0:
            warn(
                "webkit2gtk-4.1 not detected — the build will fail "
                "(Arch: sudo pacman -S webkit2gtk-4.1)"
            )
        else:
            success("webkit2gtk-4.1 found")


# ── Native (SwiftUI) ──
def build_native() -> Path:
    """Build the macOS app in Release and return the bundle.

    Three commands rather than one because each produces the next one's input.
    `build-rust.sh` compiles `leogit-ffi` and generates the Swift bindings;
    XcodeGen resolves `sources:` at generation time, so on a clean tree the
    generated Swift has to exist before the project is written or the build is
    planned around a file that is not there yet; then xcodebuild, whose own
    pre-build phase runs `build-rust.sh` again — cheaply, since cargo does its
    own change detection.

    A *clean* build, unlike `just mac-build`'s incremental one: an incremental
    Release build re-copies the SPM resource bundles without re-signing the app
    around them, leaving a bundle whose seal no longer matches its contents.
    Gatekeeper refuses that. `clean` rather than deleting the derived-data
    directory outright, so the resolved packages under `SourcePackages` survive
    and SwiftTerm is not re-fetched.
    """
    run(
        [str(MAC_DIR / "scripts" / "build-rust.sh"), "release"],
        cwd=MAC_DIR,
        what="Building leogit-ffi",
    )
    run(["xcodegen", "generate"], cwd=MAC_DIR, what="xcodegen")
    run(
        [
            "xcodebuild",
            "-project",
            "LeoGit.xcodeproj",
            "-scheme",
            "LeoGit",
            "-configuration",
            "Release",
            "-derivedDataPath",
            "build",
            # Pin the version the bundle reports to the one the version files
            # hold, so an Info.plist can never disagree with the artifact named
            # after it. XcodeGen already wrote this value into project.yml's
            # info block; passing it again makes the build fail loudly rather
            # than silently ship a stale one if the two ever part company.
            f"MARKETING_VERSION={read_version()}",
            "clean",
            "build",
        ],
        cwd=MAC_DIR,
        what="Build",
    )
    if not NATIVE_APP.is_dir():
        error(f"xcodebuild did not produce {NATIVE_APP}")
    verify_signature(NATIVE_APP)
    success(f"Built {NATIVE_APP.name}")
    return NATIVE_APP


# ── Tauri ──
def build_tauri() -> Path:
    """Build the Tauri bundle for this host and return it."""
    target_os = host_os()
    bundle_target = TAURI_BUNDLE_TARGET[target_os]
    env: dict[str, str] = {}

    if target_os == "linux":
        # linuxdeploy, the AppImage tooling Tauri downloads, needs two
        # workarounds on current Linux. Set here rather than in a shell profile
        # so release builds are reproducible on any machine.
        #   NO_STRIP — linuxdeploy bundles an old `strip` that aborts on the
        #     `.relr.dyn` section modern binutils emits ("unknown type [0x13]").
        #   APPIMAGE_EXTRACT_AND_RUN — run its nested plugin AppImages by
        #     extracting rather than FUSE-mounting, which is flaky when nested.
        env = {"NO_STRIP": "true", "APPIMAGE_EXTRACT_AND_RUN": "1"}
        _require_pixbuf_dir()
    elif target_os == "windows":
        # Tauri drops one <product>_<version>_<arch>-setup.exe per build into
        # bundle/nsis and never prunes. Wipe first so the only installer left
        # afterwards is this build's.
        shutil.rmtree(TAURI_BUNDLE_DIR / "nsis", ignore_errors=True)

    info(f"Bundling the Tauri client ({bundle_target})")
    run(
        ["pnpm", "install", "--frozen-lockfile"],
        cwd=TAURI_DIR,
        what="pnpm install",
    )
    run(
        ["pnpm", "tauri", "build", "--bundles", bundle_target],
        cwd=TAURI_DIR,
        what="tauri build",
        env=env,
    )

    if target_os == "linux":
        _drop_bundled_gio_module()

    if target_os == "macOS":
        bundle = TAURI_BUNDLE_DIR / "macos" / BUNDLE_NAME[Client.TAURI]
        if not bundle.is_dir():
            error(f"tauri build did not produce {bundle}")
        # Ad-hoc re-sign over Tauri's own signature. No Developer ID; the
        # installer strips quarantine instead.
        signed, _ = capture(["codesign", "--force", "--deep", "--sign", "-", str(bundle)])
        if signed != 0:
            warn("codesign failed (continuing — the bundle is still usable)")
        else:
            verify_signature(bundle)
    elif target_os == "windows":
        bundle = _only_file(TAURI_BUNDLE_DIR / "nsis", "*-setup.exe", "an NSIS installer")
    else:
        # AppImages are self-contained and unsigned.
        bundle = _only_file(TAURI_BUNDLE_DIR / "appimage", "*.AppImage", "an AppImage")

    success(f"Built {bundle.name}")
    return bundle


def _drop_bundled_gio_module() -> None:
    """Remove the GIO TLS module linuxdeploy's gtk plugin bundles, then repack.

    The plugin ends by copying `libgiognutls.so` into the AppDir with a raw
    `cp --parents` and pointing `GIO_EXTRA_MODULES` at it. That copy bypasses
    linuxdeploy's dependency resolution, so the module's own chain
    (libgnutls -> libnettle/libhogweed -> libgmp) is only partly bundled:
    `libgmp.so.10` is absent and resolves against the host mid-`dlopen`.

    GTK reaches that module on the very first window: creating an
    ApplicationWindow calls `gtk_css_provider_load_from_path`, which calls
    `g_vfs_get_default`, which dlopens every GIO module it can find. Loading
    this half-bundled one segfaults inside `ld.so`, so the app dies during
    `create_window` before it can paint.

    Dropping the module leaves GIO to use the host's own, which matches the
    host's GLib. LeoGit talks to git over subprocesses and libgit2, never
    through GIO's network VFS, so nothing here needs a bundled TLS backend.
    """
    appimage_dir = TAURI_BUNDLE_DIR / "appimage"
    appdirs = sorted(appimage_dir.glob("*.AppDir")) if appimage_dir.is_dir() else []
    if len(appdirs) != 1:
        error(f"Expected exactly one *.AppDir in {appimage_dir}, found {len(appdirs)}")
    appdir = appdirs[0]

    gio_modules = appdir / "usr" / "lib" / "gio"
    hook = appdir / "apprun-hooks" / "linuxdeploy-plugin-gtk.sh"
    if not gio_modules.is_dir() and "GIO_EXTRA_MODULES" not in hook.read_text():
        return  # Upstream fixed it; nothing to strip and nothing to repack.

    shutil.rmtree(gio_modules, ignore_errors=True)
    hook.write_text(
        "".join(
            line
            for line in hook.read_text().splitlines(keepends=True)
            if not line.startswith("export GIO_EXTRA_MODULES=")
        )
    )

    # The AppImage Tauri just wrote still holds the old AppDir, so rebuild it
    # from the patched tree with the same plugin Tauri itself invokes.
    packager = Path.home() / ".cache" / "tauri" / "linuxdeploy-plugin-appimage.AppImage"
    if not packager.is_file():
        error(f"Cannot repack the AppImage: {packager} is missing")
    image = _only_file(TAURI_BUNDLE_DIR / "appimage", "*.AppImage", "an AppImage")
    info(f"Repacking {image.name} without the broken GIO module")
    run(
        [str(packager), f"--appdir={appdir.name}"],
        cwd=appdir.parent,
        what="AppImage repack",
        env={
            "APPIMAGE_EXTRACT_AND_RUN": "1",
            "NO_STRIP": "true",
            "ARCH": platform.machine(),
            "OUTPUT": image.name,
        },
    )


def _require_pixbuf_dir() -> None:
    """linuxdeploy's gtk plugin copies gdk-pixbuf's loader dir unconditionally.

    On current Arch that directory is gone — gdk-pixbuf has its loaders built
    in and librsvg dropped its pixbuf loader — so the copy aborts the plugin.
    An empty directory satisfies it; LeoGit renders only PNG icons.
    """
    code, pixbuf = capture(
        ["pkg-config", "--variable=gdk_pixbuf_binarydir", "gdk-pixbuf-2.0"]
    )
    if code != 0 or not pixbuf or Path(pixbuf).is_dir():
        return
    error(
        "gdk-pixbuf loader dir missing — linuxdeploy's gtk plugin needs it. "
        f"Create it once (empty is fine):\n"
        f"    sudo mkdir -p {pixbuf}/loaders\n"
        f"    gdk-pixbuf-query-loaders | sudo tee {pixbuf}/loaders.cache >/dev/null"
    )


def _only_file(directory: Path, glob: str, what: str) -> Path:
    matches = sorted(directory.glob(glob)) if directory.is_dir() else []
    if not matches:
        error(f"tauri build did not produce {what} in {directory}")
    if len(matches) > 1:
        # Never silently pick one: a leftover from an earlier version would be
        # packaged and uploaded under this version's name.
        error(f"Found {len(matches)} candidates for {what} in {directory}; expected one")
    return matches[0]


# ── Build, either client ──
def build(client: Client) -> Path:
    return build_native() if client is Client.NATIVE else build_tauri()


# ── Signing ──
def verify_signature(bundle: Path) -> None:
    """Refuse to ship or install a bundle whose signature does not verify.

    The build signs ad hoc rather than with a Developer ID, but a *valid* seal
    is still the difference between an app that opens and one macOS refuses, so
    it is checked here rather than discovered by whoever downloads the artifact.
    """
    result = subprocess.run(
        # --verbose=2 so a failure names the file that broke the seal; on
        # success the extra chatter goes to stderr and is discarded either way.
        ["codesign", "--verify", "--deep", "--strict", "--verbose=2", str(bundle)],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        detail = " — ".join(
            line.replace(f"{bundle}: ", "") for line in result.stderr.strip().splitlines()
        )
        error(f"{bundle.name} does not verify: {detail or 'unknown'}")
    success("Signature verifies")


def bundle_version(bundle: Path) -> str:
    """`CFBundleShortVersionString` of a built macOS bundle."""
    code, value = capture(
        [
            "/usr/libexec/PlistBuddy",
            "-c",
            "Print :CFBundleShortVersionString",
            str(bundle / "Contents" / "Info.plist"),
        ]
    )
    if code != 0:
        error(f"Could not read the version of {bundle}")
    return value


# ── Packaging ──
def package(built: Path, version: str) -> Path:
    """Put the built bundle into `dist/` under its release artifact name."""
    DIST_DIR.mkdir(parents=True, exist_ok=True)
    destination = DIST_DIR / artifact_name(version)
    destination.unlink(missing_ok=True)

    if host_os() == "macOS":
        reported = bundle_version(built)
        if reported != version:
            error(f"The built bundle reports {reported}, expected {version}")
        # `ditto -c -k`, not `zip`: it preserves the extended attributes and the
        # code signature that `zip` flattens — and `install.sh` unpacks it with
        # the matching `ditto -x -k`.
        run(
            ["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", built.name, str(destination)],
            cwd=built.parent,
            what="Packaging",
        )
    else:
        # The AppImage and the NSIS installer are each a single self-contained
        # file already; renaming into dist/ is the whole job.
        shutil.copy2(built, destination)
        destination.chmod(0o755)

    size = destination.stat().st_size / 1_000_000
    success(f"Packaged: {destination.name} ({size:.0f} MB)")
    return destination


# ── Installing ──
def install_bundle(source: Path) -> Path:
    """Replace the installed macOS app with `source`, and return where it went.

    Both bundle names are removed first, not just the one being written. On the
    default case-insensitive volume `LeoGit.app` and `leogit.app` are one path
    and the second removal is a no-op; on a case-sensitive one they are two, and
    leaving the other behind would give `/Applications` two LeoGits with the
    same purpose and let Launch Services answer the name with either.
    """
    verify_signature(source)
    destination = APPLICATIONS / source.name

    for name in BUNDLE_NAME.values():
        existing = APPLICATIONS / name
        if not existing.exists():
            continue
        try:
            shutil.rmtree(existing)
        except OSError as exc:
            error(f"Could not replace {existing}: {exc}")
        warn(f"Replaced the existing {existing}")

    # ditto rather than cp: it carries extended attributes and the bundle's
    # seal across intact, so the ad-hoc signature still verifies and nothing
    # has to be re-signed here.
    run(["ditto", str(source), str(destination)], what="Copy")

    # The quarantine flag only, not every xattr: the app is ad-hoc signed
    # rather than notarized, so a copy that arrived as a download would
    # otherwise meet "the developer cannot be verified", while a blanket clear
    # is a needless swing at the signature. A no-op for a locally built bundle.
    quietly(["xattr", "-dr", "com.apple.quarantine", str(destination)])

    # Register with Launch Services so Spotlight and Launchpad know about the
    # new bundle instead of waiting for Finder to notice it — and so `open -a`
    # resolves it, which is how the `leogit` shell function reaches the app.
    if LSREGISTER.is_file():
        quietly([str(LSREGISTER), "-f", str(destination)])

    success(f"Installed: {destination}")
    return destination
