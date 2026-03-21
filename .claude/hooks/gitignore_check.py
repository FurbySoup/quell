"""PostToolUse hook — checks if newly written files should be gitignored.

Fires after Write/Edit tool use. If the file matches patterns that are
commonly sensitive or shouldn't be in a public repo, prints a warning.

Never blocks — always exits 0.
"""
import json
import os
import subprocess
import sys

PROJECT_ROOT = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..')
)

# Patterns for files that should almost always be gitignored
SENSITIVE_EXTENSIONS = {
    '.env', '.pem', '.key', '.p12', '.pfx', '.jks',
    '.credentials', '.secret', '.token',
}

SENSITIVE_NAMES = {
    '.env', '.env.local', '.env.production', '.env.development',
    'credentials.json', 'secrets.json', 'service-account.json',
    '.npmrc', '.pypirc', '.docker/config.json',
}

# Patterns for files that are typically build/temp artifacts
ARTIFACT_PATTERNS = [
    'node_modules/',
    '__pycache__/',
    '.pyc',
    'dist/',
    '.tsbuildinfo',
    '.parcel-cache/',
    '.next/',
    '.nuxt/',
    'target/debug/',
    'target/release/',
    '*.log',
]

# Directories/files that shouldn't be in a public repo
PRIVATE_PATTERNS = [
    'testing-screenshots/',
    'research/',
    '.commit-advisor-state.json',
]


def _is_tracked_by_git(file_path):
    """Check if a file is already tracked by git."""
    try:
        result = subprocess.run(
            ['git', 'ls-files', '--error-unmatch', file_path],
            capture_output=True, text=True, timeout=3,
            cwd=PROJECT_ROOT
        )
        return result.returncode == 0
    except (subprocess.TimeoutExpired, OSError):
        return False


def _is_gitignored(file_path):
    """Check if a file is already covered by .gitignore."""
    try:
        result = subprocess.run(
            ['git', 'check-ignore', '-q', file_path],
            capture_output=True, text=True, timeout=3,
            cwd=PROJECT_ROOT
        )
        return result.returncode == 0
    except (subprocess.TimeoutExpired, OSError):
        return False


def main():
    try:
        try:
            hook_input = json.loads(sys.stdin.read())
        except (json.JSONDecodeError, ValueError):
            return

        tool_name = hook_input.get('tool_name', '')
        if tool_name not in ('Write', 'Edit'):
            return

        tool_input = hook_input.get('tool_input', {})
        file_path = tool_input.get('file_path', '')
        if not file_path:
            return

        file_path = os.path.normpath(file_path)
        project_norm = os.path.normpath(PROJECT_ROOT)

        if not file_path.startswith(project_norm):
            return

        rel_path = os.path.relpath(file_path, project_norm).replace('\\', '/')
        basename = os.path.basename(rel_path).lower()
        _, ext = os.path.splitext(basename)

        # Skip if already gitignored
        if _is_gitignored(file_path):
            return

        warnings = []

        # Check sensitive file extensions
        if ext in SENSITIVE_EXTENSIONS:
            warnings.append(
                f"'{rel_path}' has sensitive extension '{ext}' — "
                "should this be in .gitignore?"
            )

        # Check sensitive file names
        if basename in SENSITIVE_NAMES:
            warnings.append(
                f"'{rel_path}' matches a known sensitive filename — "
                "should this be in .gitignore?"
            )

        # Check artifact patterns
        for pattern in ARTIFACT_PATTERNS:
            if pattern in rel_path:
                warnings.append(
                    f"'{rel_path}' looks like a build artifact (matches '{pattern}') — "
                    "should this be in .gitignore?"
                )
                break

        # Check private/internal patterns
        for pattern in PRIVATE_PATTERNS:
            if rel_path.startswith(pattern) or ('/' + pattern) in rel_path:
                warnings.append(
                    f"'{rel_path}' is in a private directory — "
                    "verify .gitignore covers it before committing."
                )
                break

        # Check for common secrets in content (Write only, not Edit)
        if tool_name == 'Write':
            content = tool_input.get('content', '')
            if content:
                lower_content = content[:2000].lower()  # Only scan first 2KB
                secret_indicators = [
                    ('api_key', 'API key'),
                    ('api-key', 'API key'),
                    ('secret_key', 'secret key'),
                    ('private_key', 'private key'),
                    ('-----begin', 'PEM certificate/key'),
                    ('password', 'password'),
                    ('aws_access_key', 'AWS credentials'),
                    ('sk_live_', 'Stripe live key'),
                    ('sk-', 'API secret key'),
                ]
                for indicator, label in secret_indicators:
                    if indicator in lower_content:
                        # Don't flag if it's clearly a placeholder or config example
                        if not any(safe in lower_content for safe in [
                            'example', 'placeholder', 'your_', 'xxx', '<',
                            'todo', 'replace', 'template',
                        ]):
                            warnings.append(
                                f"'{rel_path}' may contain a {label} — "
                                "verify this file should be committed publicly."
                            )
                            break

        if warnings:
            print(
                "\n[gitignore-check] New/modified file review:"
                + ''.join(f"\n  ⚠ {w}" for w in warnings),
                file=sys.stderr,
                flush=True
            )

    except Exception:
        pass
    finally:
        sys.exit(0)


if __name__ == '__main__':
    main()
