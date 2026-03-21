"""PreToolUse hook — checks branch/phase context before file edits.

Fires before Edit or Write on project source files. Checks:
1. Current git branch (master = Phase 1 CLI, phase-2 = Phase 2 GUI)
2. Whether the file being edited matches the expected phase
3. Git status (uncommitted changes, detached HEAD)

Prints advisory to stderr. Never blocks (exit 0).
"""
import json
import os
import subprocess
import sys

PROJECT_ROOT = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..')
)

# Phase 2 paths — files that should only be edited on phase-2 branch
PHASE2_PATHS = {'gui/', 'src-tauri', 'src-frontend', 'package.json', 'tsconfig.json'}

# Phase 1 core paths — CLI binary + shared engine (should primarily be on master)
PHASE1_PATHS = {'cli/', 'src/proxy', 'src/conpty', 'src/vt', 'src/history', 'src/config'}


def _get_git_info():
    """Get current branch and basic status."""
    info = {'branch': None, 'detached': False, 'dirty_count': 0}
    try:
        result = subprocess.run(
            ['git', 'branch', '--show-current'],
            capture_output=True, text=True, timeout=3,
            cwd=PROJECT_ROOT
        )
        branch = result.stdout.strip()
        if branch:
            info['branch'] = branch
        else:
            info['detached'] = True

        result = subprocess.run(
            ['git', 'status', '--porcelain'],
            capture_output=True, text=True, timeout=3,
            cwd=PROJECT_ROOT
        )
        info['dirty_count'] = len([
            l for l in result.stdout.strip().splitlines() if l.strip()
        ])
    except (subprocess.TimeoutExpired, OSError):
        pass
    return info


def main():
    try:
        try:
            hook_input = json.loads(sys.stdin.read())
        except (json.JSONDecodeError, ValueError):
            return

        tool_input = hook_input.get('tool_input', {})
        file_path = tool_input.get('file_path', '')
        if not file_path:
            return

        file_path = os.path.normpath(file_path)
        project_norm = os.path.normpath(PROJECT_ROOT)

        # Only care about files in this project
        if not file_path.startswith(project_norm):
            return

        rel_path = os.path.relpath(file_path, project_norm).replace('\\', '/')

        git_info = _get_git_info()
        branch = git_info['branch']
        warnings = []

        if git_info['detached']:
            warnings.append("HEAD is detached — you're not on any branch!")

        # Check phase mismatch
        if branch:
            is_phase2_file = any(rel_path.startswith(p) for p in PHASE2_PATHS)
            is_phase1_file = any(rel_path.startswith(p) for p in PHASE1_PATHS)

            if is_phase2_file and branch == 'master':
                warnings.append(
                    f"Editing Phase 2 file '{rel_path}' on master branch. "
                    "Phase 2 GUI work should be on the phase-2 branch."
                )
            elif is_phase1_file and branch.startswith('phase-2'):
                warnings.append(
                    f"Editing Phase 1 CLI file '{rel_path}' on phase-2 branch. "
                    "Phase 1 fixes should go on master first, then merge."
                )

        # Report dirty count if high
        if git_info['dirty_count'] > 10:
            warnings.append(
                f"{git_info['dirty_count']} uncommitted changes. "
                "Consider committing before making more edits."
            )

        if warnings:
            header = f"[branch-phase-check] Branch: {branch or 'DETACHED'} | File: {rel_path}"
            print(
                f"\n{header}\n" + '\n'.join(f"  ⚠ {w}" for w in warnings),
                file=sys.stderr,
                flush=True
            )

    except Exception:
        pass
    finally:
        sys.exit(0)


if __name__ == '__main__':
    main()
