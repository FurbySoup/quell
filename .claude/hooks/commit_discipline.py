"""PreToolUse hook — enforces git safety and Phase 2 push protection.

Fires before Bash commands. Blocks:
1. git push when Phase 2 files (gui/) would be sent to public origin —
   Phase 2 stays local until a stable release is ready
2. git push --force — too dangerous for a public repo
3. git reset --hard — destructive without confirmation

Local commits and merges are unrestricted — work locally however you like.

Prints feedback to stderr and exits 2 to block.
Exits 0 to allow.
"""
import json
import os
import re
import subprocess
import sys

PROJECT_ROOT = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..')
)

# Paths that are Phase 2 — must not be pushed to public origin
PHASE2_PATHS = ['gui/', 'src-tauri/', 'src-frontend/', 'package.json', 'tsconfig.json']


def _diff_includes_phase2():
    """Check if commits ahead of origin contain Phase 2 files."""
    try:
        # Try upstream first, fall back to origin/master
        result = subprocess.run(
            ['git', 'diff', '--name-only', '@{u}..HEAD'],
            capture_output=True, text=True, timeout=5,
            cwd=PROJECT_ROOT
        )
        if result.returncode != 0:
            result = subprocess.run(
                ['git', 'diff', '--name-only', 'origin/master..HEAD'],
                capture_output=True, text=True, timeout=5,
                cwd=PROJECT_ROOT
            )
        if result.returncode != 0:
            return False, []

        changed = result.stdout.strip().splitlines()
        phase2_files = [
            f for f in changed
            if any(f.startswith(p) or f == p.rstrip('/') for p in PHASE2_PATHS)
        ]
        return bool(phase2_files), phase2_files
    except (subprocess.TimeoutExpired, OSError):
        return False, []


def main():
    try:
        try:
            hook_input = json.loads(sys.stdin.read())
        except (json.JSONDecodeError, ValueError):
            return

        tool_name = hook_input.get('tool_name', '')
        if tool_name != 'Bash':
            return

        command = hook_input.get('tool_input', {}).get('command', '')
        if not command:
            return

        # --- Block force push ---
        if re.search(r'git\s+push\s+.*(-f|--force)', command):
            print(
                "[push-guard] BLOCKED: force push is not allowed on a public repo.\n"
                "  Use regular push or rebase instead.",
                file=sys.stderr, flush=True
            )
            sys.exit(2)

        # --- Block git reset --hard ---
        if re.search(r'git\s+reset\s+--hard', command):
            print(
                "[push-guard] BLOCKED: git reset --hard is destructive.\n"
                "  Use git stash or git checkout <file> for targeted reverts.",
                file=sys.stderr, flush=True
            )
            sys.exit(2)

        # --- Block pushing Phase 2 code to public origin ---
        if re.search(r'\bgit\s+push\b', command):
            has_phase2, files = _diff_includes_phase2()
            if has_phase2:
                file_list = '\n'.join(f'    {f}' for f in files[:10])
                print(
                    "[push-guard] BLOCKED: push includes Phase 2 files.\n"
                    "  Phase 2 (GUI) stays local until a stable release.\n"
                    f"  Phase 2 files in diff:\n{file_list}\n"
                    "  To push Phase 1 changes only, revert or stash Phase 2 files first.",
                    file=sys.stderr, flush=True
                )
                sys.exit(2)

    except SystemExit:
        raise  # Let sys.exit(2) propagate for blocking
    except Exception:
        pass
    sys.exit(0)


if __name__ == '__main__':
    main()
