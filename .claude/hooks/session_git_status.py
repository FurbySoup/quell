"""SessionStart hook — injects git repo state as context.

Fires at the start of every session. Outputs JSON with additionalContext
so Claude sees the repo state before the user's first prompt.

Checks: current branch, unpushed commits, uncommitted changes,
stale branches, and remote sync status.

Never blocks — always exits 0.
"""
import json
import subprocess
import sys
import os

PROJECT_ROOT = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..')
)


def _run_git(*args):
    """Run a git command and return stdout, or empty string on failure."""
    try:
        result = subprocess.run(
            ['git'] + list(args),
            capture_output=True, text=True, timeout=5,
            cwd=PROJECT_ROOT
        )
        return result.stdout.strip() if result.returncode == 0 else ''
    except (subprocess.TimeoutExpired, OSError):
        return ''


def main():
    try:
        # Read hook input (SessionStart provides session_id, source, etc.)
        try:
            json.loads(sys.stdin.read())
        except (json.JSONDecodeError, ValueError):
            pass

        lines = []

        # Current branch
        branch = _run_git('branch', '--show-current')
        if not branch:
            lines.append('HEAD is DETACHED — not on any branch')
        else:
            lines.append(f'Branch: {branch}')

        # Unpushed commits
        unpushed = _run_git('log', '--oneline', '@{u}..HEAD')
        if unpushed:
            count = len(unpushed.splitlines())
            lines.append(f'{count} UNPUSHED commit(s) — push before ending session:')
            for line in unpushed.splitlines()[:5]:
                lines.append(f'  {line}')
            if count > 5:
                lines.append(f'  ... and {count - 5} more')
        else:
            lines.append('Up to date with remote')

        # Uncommitted changes
        status = _run_git('status', '--porcelain')
        if status:
            count = len([l for l in status.splitlines() if l.strip()])
            lines.append(f'{count} uncommitted change(s) in working tree')

        # Stale branches (not updated in 7+ days, excluding master)
        stale = []
        branches_raw = _run_git(
            'for-each-ref', '--sort=-committerdate',
            '--format=%(refname:short) %(committerdate:relative)',
            'refs/heads'
        )
        if branches_raw:
            for line in branches_raw.splitlines():
                parts = line.split(' ', 1)
                if len(parts) == 2:
                    bname, age = parts
                    if bname != 'master' and any(
                        w in age for w in ['week', 'month', 'year']
                    ):
                        stale.append(f'  {bname} ({age})')
        if stale:
            lines.append(f'{len(stale)} stale branch(es):')
            lines.extend(stale[:5])

        # Last commit info
        last = _run_git('log', '-1', '--format=%h %s (%ar)')
        if last:
            lines.append(f'Last commit: {last}')

        if not lines:
            return

        context = '[session-git-status] ' + ' | '.join(lines[:3])
        if len(lines) > 3:
            context += '\n' + '\n'.join(lines[3:])

        output = {
            "hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": context
            }
        }
        print(json.dumps(output), flush=True)

    except Exception:
        pass
    finally:
        sys.exit(0)


if __name__ == '__main__':
    main()
