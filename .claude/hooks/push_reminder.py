"""Stop hook — reminds about unpushed commits when Claude finishes.

Fires when Claude stops responding. If there are unpushed commits,
prints a reminder to stderr. Checks stop_hook_active to avoid loops.

Never blocks — always exits 0.
"""
import json
import subprocess
import sys
import os

PROJECT_ROOT = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..')
)


def main():
    try:
        try:
            hook_input = json.loads(sys.stdin.read())
        except (json.JSONDecodeError, ValueError):
            return

        # Prevent infinite loops
        if hook_input.get('stop_hook_active', False):
            return

        # Check for unpushed commits
        try:
            result = subprocess.run(
                ['git', 'log', '--oneline', '@{u}..HEAD'],
                capture_output=True, text=True, timeout=5,
                cwd=PROJECT_ROOT
            )
            if result.returncode != 0 or not result.stdout.strip():
                return

            count = len(result.stdout.strip().splitlines())
        except (subprocess.TimeoutExpired, OSError):
            return

        # Check for uncommitted changes too
        try:
            result = subprocess.run(
                ['git', 'status', '--porcelain'],
                capture_output=True, text=True, timeout=5,
                cwd=PROJECT_ROOT
            )
            uncommitted = len([
                l for l in result.stdout.strip().splitlines() if l.strip()
            ]) if result.returncode == 0 else 0
        except (subprocess.TimeoutExpired, OSError):
            uncommitted = 0

        parts = []
        if count:
            parts.append(f'{count} unpushed commit(s)')
        if uncommitted:
            parts.append(f'{uncommitted} uncommitted change(s)')

        if parts:
            print(
                f"\n[push-reminder] {' and '.join(parts)} — "
                "consider pushing before ending your session.",
                file=sys.stderr,
                flush=True
            )

    except Exception:
        pass
    finally:
        sys.exit(0)


if __name__ == '__main__':
    main()
