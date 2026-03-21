"""TaskCompleted hook — reminds about live-proving requirements.

Fires when a task is marked as completed. Checks if the task looks like
a feature/fix (not just docs/config) and reminds to categorize live-proving
steps as automated, automatable, or requiring human GUI interaction.

Never blocks — always exits 0, prints reminder to stderr.
"""
import json
import sys

# Keywords suggesting a task is just config/docs, not a feature needing live-proving
SKIP_KEYWORDS = [
    'readme', 'docs', 'documentation', 'comment', 'typo', 'rename',
    'hook', 'config', 'setting', 'memory', 'roadmap', 'plan',
    'gitignore', 'clippy', 'lint', 'format',
]


def main():
    try:
        try:
            hook_input = json.loads(sys.stdin.read())
        except (json.JSONDecodeError, ValueError):
            return

        subject = hook_input.get('task_subject', '').lower()
        description = (hook_input.get('task_description', '') or '').lower()
        combined = subject + ' ' + description

        # Skip non-feature tasks
        if any(kw in combined for kw in SKIP_KEYWORDS):
            return

        print(
            "\n[live-proving-reminder] Feature task completed. Before closing:\n"
            "  1. Which live-proving steps are AUTOMATED? (replay tests, CI)\n"
            "  2. Which are AUTOMATABLE? (could be scripted but aren't yet)\n"
            "  3. Which need HUMAN interaction with the GUI/terminal?\n"
            "  4. Have all three categories been addressed?\n"
            "\n"
            "  Tier 1: VT recording + replay (automated)\n"
            "  Tier 2: Scroll stability probe (automatable)\n"
            "  Tier 3: Visual/interactive walkthrough (human required)\n",
            file=sys.stderr,
            flush=True
        )

    except Exception:
        pass
    finally:
        sys.exit(0)


if __name__ == '__main__':
    main()
