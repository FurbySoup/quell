"""PreToolUse hook — enforces feature branch workflow.

Fires before Bash commands. Blocks:
1. git commit directly on master — must use a feature branch
2. git push --force — too dangerous
3. git reset --hard — destructive without confirmation

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


def _get_branch():
    """Get current branch name."""
    try:
        result = subprocess.run(
            ['git', 'branch', '--show-current'],
            capture_output=True, text=True, timeout=3,
            cwd=PROJECT_ROOT
        )
        return result.stdout.strip() if result.returncode == 0 else None
    except (subprocess.TimeoutExpired, OSError):
        return None


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
                "[commit-discipline] BLOCKED: force push is not allowed.\n"
                "  Use regular push or rebase instead.",
                file=sys.stderr, flush=True
            )
            sys.exit(2)

        # --- Block git reset --hard ---
        if re.search(r'git\s+reset\s+--hard', command):
            print(
                "[commit-discipline] BLOCKED: git reset --hard is destructive.\n"
                "  Use git stash or git checkout <file> for targeted reverts.",
                file=sys.stderr, flush=True
            )
            sys.exit(2)

        # --- Block direct commits to master ---
        if re.search(r'\bgit\s+commit\b', command):
            branch = _get_branch()
            if branch in ('master', 'main'):
                print(
                    "[commit-discipline] BLOCKED: cannot commit directly to master.\n"
                    "  Create a feature branch first:\n"
                    "    git checkout -b feature/your-feature\n"
                    "  Then commit, push, and create a PR.",
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
