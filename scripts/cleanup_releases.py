#!/usr/bin/env python3
"""Deletes every GitHub release older than the latest one.

    python3 scripts/cleanup_releases.py             # asks first
    python3 scripts/cleanup_releases.py --dry-run   # show what would go
    python3 scripts/cleanup_releases.py --yes       # no prompt

Only the newest release is kept. Its tag is what `install.sh` and the in-app
update check both resolve, so the older ones carry artifacts nothing will ever
download again — while still costing storage and making the releases page a
scroll.

The git tags go with the releases by default: a tag left behind after its
release is deleted is a commit pointer with nothing attached, and
`deploy_release.py` reuses an existing tag rather than failing on it, so a
stray one would silently attach a future release to an old commit. Pass
--keep-tags to keep them anyway.
"""

from __future__ import annotations

import argparse
import json

from _common import (
    GREEN,
    NC,
    RED,
    REPO,
    Steps,
    capture,
    error,
    finish,
    quietly,
    require_gh_auth,
    require_tools,
    success,
    warn,
)

step = Steps(3)


def releases(limit: int) -> list[str]:
    """Every release tag, newest first — the order `gh` already lists them in."""
    code, payload = capture(
        ["gh", "release", "list", "--limit", str(limit), "--repo", REPO, "--json", "tagName"]
    )
    if code != 0:
        error(f"Failed to list releases for {REPO}")
    try:
        listed = json.loads(payload)
    except json.JSONDecodeError as exc:
        error(f"Could not parse the release list: {exc}")
    return [entry["tagName"] for entry in listed if entry.get("tagName")]


def confirm(count: int) -> None:
    try:
        answer = input(f"  Delete these {count} releases? [y/N]: ")
    except (KeyboardInterrupt, EOFError):
        print()
        error("Cancelled")
    if answer.strip().lower() not in ("y", "yes"):
        error("Cancelled")


def delete(tag: str, *, with_tag: bool) -> bool:
    """Delete one release. Returns whether it went."""
    command = ["gh", "release", "delete", tag, "--yes", "--repo", REPO]
    if with_tag:
        command.append("--cleanup-tag")
    code, _ = capture(command)
    if code == 0:
        return True
    if not with_tag:
        return False
    # A protected or already-deleted tag fails the combined call while the
    # release itself would have gone fine. Retry without it rather than leaving
    # a release standing because of its tag.
    warn(f"{tag}: could not remove the tag, deleting the release alone")
    code, _ = capture(["gh", "release", "delete", tag, "--yes", "--repo", REPO])
    return code == 0


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Delete every GitHub release older than the latest one."
    )
    parser.add_argument(
        "-d", "--dry-run", action="store_true", help="show what would be deleted, delete nothing"
    )
    parser.add_argument("-y", "--yes", action="store_true", help="skip the confirmation prompt")
    parser.add_argument(
        "-l", "--limit", type=int, default=100, help="how many releases to inspect (default: 100)"
    )
    parser.add_argument(
        "--keep-tags",
        dest="cleanup_tag",
        action="store_false",
        help="leave the git tags behind when deleting their releases",
    )
    args = parser.parse_args()

    # ── Step 1: Validate prerequisites ──
    step("Validating prerequisites")
    require_tools("gh")
    require_gh_auth()
    code, _ = capture(["gh", "repo", "view", REPO])
    if code != 0:
        error(f"Cannot access {REPO}")
    success(f"Repository access verified: {REPO}")

    # ── Step 2: Identify releases ──
    step(f"Identifying releases for {REPO}")
    tags = releases(args.limit)
    if not tags:
        error(f"No releases found for {REPO}")
    if len(tags) == 1:
        finish(f"Nothing to delete for {REPO}", f"{tags[0]} is the only release")
        return

    keep, doomed = tags[0], tags[1:]
    print(f"  Keeping the latest release: {GREEN}{keep}{NC}")
    print(f"  Deleting {RED}{len(doomed)}{NC} older releases:")
    for tag in doomed:
        print(f"    - {tag}")

    # ── Step 3: Execute cleanup ──
    step("Executing cleanup")
    if args.dry_run:
        warn("Dry run — nothing was deleted")
        return
    if not args.yes:
        confirm(len(doomed))

    failed = [tag for tag in doomed if not delete(tag, with_tag=args.cleanup_tag)]
    for tag in doomed:
        if tag not in failed:
            success(f"Deleted {tag}")
    if failed:
        error(f"Could not delete: {', '.join(failed)}")

    if args.cleanup_tag:
        # `--cleanup-tag` removes the remote tag; the local clone keeps its own
        # copy until it is told otherwise, and a stale local tag is exactly what
        # makes `deploy_release.py` reuse one instead of creating it.
        quietly(["git", "fetch", "--prune", "--prune-tags", "origin"])

    finish(f"Cleanup complete for {REPO}", f"Kept {keep}")


if __name__ == "__main__":
    main()
