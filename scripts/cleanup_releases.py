#!/usr/bin/env python3
import sys
import os
import subprocess
import shutil
import json
import argparse
import re

# Deletes older releases than the latest one for the LeoGit repository:
#   1. Validates prerequisites (gh CLI installed and authenticated)
#   2. Fetches the list of releases
#   3. Identifies the latest release (to keep) and all older releases (to delete)
#   4. Deletes older releases and optionally their git tags (both local/remote)
#
# Usage: python3 scripts/cleanup_releases.py [options]
#   By default, it will prompt for confirmation before deleting.
#   Run with --dry-run to preview actions safely.

# ── Colors ──
RED = '\033[0;31m'
GREEN = '\033[0;32m'
YELLOW = '\033[1;33m'
BLUE = '\033[0;34'
CYAN = '\033[0;36m'
NC = '\033[0m'

# Add missing color escape character fix
BLUE = '\033[0;34m'

TOTAL_STEPS = 3

def step(step_num, title):
    print(f"\n{BLUE}[{step_num}/{TOTAL_STEPS}]{NC} {CYAN}{title}{NC}")

def success(msg):
    print(f"  {GREEN}✓ {msg}{NC}")

def warn(msg):
    print(f"  {YELLOW}⚠ {msg}{NC}")

def error(msg):
    print(f"  {RED}✗ {msg}{NC}", file=sys.stderr)
    sys.exit(1)

def command_exists(name):
    return shutil.which(name) is not None

def detect_repo():
    if not command_exists("git"):
        return "LeoManrique/leogit"
    
    # Check if inside git work tree
    res = subprocess.run(["git", "rev-parse", "--is-inside-work-tree"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
    if res.returncode != 0:
        return "LeoManrique/leogit"
    
    # Get origin URL
    res = subprocess.run(["git", "remote", "get-url", "origin"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
    if res.returncode != 0:
        return "LeoManrique/leogit"
    
    url = res.stdout.strip()
    match = re.search(r'github\.com[:/]([^/]+/[^/.]+?)(?:\.git)?$', url)
    if match:
        return match.group(1)
        
    return "LeoManrique/leogit"

def main():
    parser = argparse.ArgumentParser(description="Deletes older GitHub releases than the latest one.")
    parser.add_argument("-d", "--dry-run", action="store_true", help="Show what would be deleted without actually deleting")
    parser.add_argument("-y", "--yes", action="store_true", help="Skip interactive confirmation prompt")
    parser.add_argument("-r", "--repo", help="Specify the GitHub repository (owner/repo) (default: auto-detect)")
    parser.add_argument("-l", "--limit", type=int, default=100, help="Maximum number of releases to inspect (default: 100)")
    parser.add_argument("--no-cleanup-tag", dest="cleanup_tag", action="store_false", help="Do not delete the git tags associated with the releases")
    
    args = parser.parse_args()
    
    # Resolve repository
    repo = args.repo if args.repo else detect_repo()
    
    # Change working directory to project root
    script_dir = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.dirname(script_dir)
    os.chdir(project_root)
    
    # ── Step 1: Validate prerequisites ──
    step(1, "Validating prerequisites")
    
    if not command_exists("gh"):
        error("gh CLI is not installed")
    success("gh CLI found")
    
    auth_check = subprocess.run(["gh", "auth", "status"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
    if auth_check.returncode != 0:
        error("gh CLI not authenticated. Run: gh auth login")
    success("gh authenticated")
    
    repo_check = subprocess.run(["gh", "repo", "view", repo], stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
    if repo_check.returncode != 0:
        error(f"Cannot access repository: {repo}")
    success(f"Repository access verified: {repo}")
    
    # ── Step 2: Identify releases ──
    step(2, f"Identifying releases for {repo}")
    
    cmd = ["gh", "release", "list", "--limit", str(args.limit), "--repo", repo, "--json", "tagName"]
    list_check = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
    if list_check.returncode != 0:
        error(f"Failed to list releases: {list_check.stderr.strip()}")
        
    try:
        releases = json.loads(list_check.stdout)
    except Exception as e:
        error(f"Failed to parse release JSON output: {e}")
        
    if not releases:
        error(f"No releases found for repository: {repo}")
        
    tags = [r["tagName"] for r in releases if "tagName" in r]
    
    if len(tags) <= 1:
        success(f"Only one release exists ({tags[0]}). Nothing to delete.")
        sys.exit(0)
        
    keep_tag = tags[0]
    delete_tags = tags[1:]
    
    print(f"  Keeping latest release: {GREEN}{keep_tag}{NC}")
    print(f"  The following {RED}{len(delete_tags)}{NC} older releases will be deleted:")
    for tag in delete_tags:
        print(f"    - {tag}")
        
    # ── Step 3: Execute cleanup ──
    step(3, "Executing cleanup")
    
    if args.dry_run:
        warn("Dry-run active. No releases were deleted.")
        sys.exit(0)
        
    if not args.yes:
        try:
            confirm = input(f"  {YELLOW}⚠{NC} Are you sure you want to delete these {len(delete_tags)} releases? [y/N]: ")
        except (KeyboardInterrupt, EOFError):
            print()
            error("Cleanup cancelled by user.")
        if not confirm.strip().lower() in ['y', 'yes']:
            error("Cleanup cancelled by user.")
            
    # Set gh release delete flags
    delete_flags = ["--yes"]
    if args.cleanup_tag:
        delete_flags.append("--cleanup-tag")
        
    for tag in delete_tags:
        print(f"  Deleting release {tag}... ", end="", flush=True)
        del_cmd = ["gh", "release", "delete", tag] + delete_flags + ["--repo", repo]
        del_check = subprocess.run(del_cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
        if del_check.returncode == 0:
            print(f"{GREEN}Deleted{NC}")
        else:
            # Retry without cleanup tag if it failed, or show error
            warn("Failed to delete with tag. Retrying release only...")
            print(f"  Deleting release {tag} (release only)... ", end="", flush=True)
            retry_cmd = ["gh", "release", "delete", tag, "--yes", "--repo", repo]
            retry_check = subprocess.run(retry_cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
            if retry_check.returncode == 0:
                print(f"{GREEN}Deleted (release only){NC}")
            else:
                print(f"{RED}Failed{NC}")
                
    success(f"Cleanup complete. Kept latest release: {keep_tag}")
    print(f"\n{GREEN}═══ Cleanup complete for {repo} ═══{NC}")

if __name__ == "__main__":
    main()
