"""PreToolUse hook — runs cargo test + clippy before git commits.

Fires before Bash tool use. Detects git commit commands and runs
cargo test and cargo clippy first. If either fails, blocks the commit
with exit code 2 and feedback on stderr.

This enforces CLAUDE.md rules:
  - "Run cargo test — all tests must pass"
  - "Run cargo clippy — no warnings"
"""
import json
import os
import re
import subprocess
import sys

PROJECT_ROOT = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..')
)

# Patterns that indicate a git commit is being attempted
COMMIT_PATTERNS = [
    r'\bgit\s+commit\b',
]


def _is_commit_command(command: str) -> bool:
    """Check if the bash command is a git commit."""
    for pattern in COMMIT_PATTERNS:
        if re.search(pattern, command, re.IGNORECASE):
            return True
    return False


def _run_check(cmd: list[str], label: str) -> tuple[bool, str]:
    """Run a check command and return (passed, output)."""
    try:
        result = subprocess.run(
            cmd,
            capture_output=True, text=True, timeout=120,
            cwd=PROJECT_ROOT
        )
        passed = result.returncode == 0
        output = result.stdout + result.stderr
        return passed, output.strip()
    except subprocess.TimeoutExpired:
        return False, f"{label} timed out after 120s"
    except OSError as e:
        return False, f"{label} failed to run: {e}"


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
        if not _is_commit_command(command):
            return

        # Run cargo test
        test_ok, test_output = _run_check(['cargo', 'test'], 'cargo test')
        if not test_ok:
            print(
                "[pre-commit-gate] cargo test FAILED — commit blocked.\n"
                f"Output (last 500 chars):\n{test_output[-500:]}",
                file=sys.stderr,
                flush=True
            )
            sys.exit(2)

        # Run cargo clippy
        clippy_ok, clippy_output = _run_check(
            ['cargo', 'clippy', '--all-targets', '--', '-D', 'warnings'],
            'cargo clippy'
        )
        if not clippy_ok:
            print(
                "[pre-commit-gate] cargo clippy FAILED — commit blocked.\n"
                f"Output (last 500 chars):\n{clippy_output[-500:]}",
                file=sys.stderr,
                flush=True
            )
            sys.exit(2)

        print(
            "[pre-commit-gate] cargo test + clippy passed.",
            file=sys.stderr,
            flush=True
        )

    except Exception:
        pass
    finally:
        sys.exit(0)


if __name__ == '__main__':
    main()
