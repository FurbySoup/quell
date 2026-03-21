"""Stop hook — detects circular debugging and suggests structured approach.

Fires when Claude finishes responding. Reads last_assistant_message for
patterns suggesting going in circles ("but wait", "actually", "I was wrong",
repeated attempts). If detected, suggests switching to systematic debugging.

Never blocks — always exits 0, prints suggestion to stderr.
Checks stop_hook_active to avoid infinite loops.
"""
import json
import re
import sys

# Patterns suggesting circular debugging
CORRECTION_PATTERNS = [
    r'\bbut wait\b',
    r'\bactually[,.]',
    r'\bI was wrong\b',
    r'\blet me reconsider\b',
    r'\bthat\'s not right\b',
    r'\bhmm\b.*\bactually\b',
    r'\bno[,.]?\s+that\b',
    r'\bwait[,.]?\s+(?:the|this|that|I)\b',
    r'\bI made (?:a|an) (?:mistake|error)\b',
    r'\blet me re-?(?:read|check|examine|think)\b',
    r'\bon second thought\b',
]

# Threshold: how many correction patterns before triggering
CORRECTION_THRESHOLD = 3


def main():
    try:
        try:
            hook_input = json.loads(sys.stdin.read())
        except (json.JSONDecodeError, ValueError):
            return

        # Prevent infinite loops
        if hook_input.get('stop_hook_active', False):
            return

        message = hook_input.get('last_assistant_message', '')
        if not message:
            return

        # Count correction patterns
        count = 0
        for pattern in CORRECTION_PATTERNS:
            matches = re.findall(pattern, message, re.IGNORECASE)
            count += len(matches)

        if count >= CORRECTION_THRESHOLD:
            print(
                f"\n[systematic-debugging] Detected {count} correction/backtracking patterns.\n"
                "  This looks like a complex bug with circular reasoning. Consider:\n"
                "\n"
                "  SYSTEMATIC DEBUGGING APPROACH:\n"
                "  1. STOP — State the exact symptom (what happens vs what should happen)\n"
                "  2. HYPOTHESIZE — List 3 possible root causes, ranked by likelihood\n"
                "  3. TEST — For each hypothesis, describe a minimal test to confirm/refute\n"
                "  4. ISOLATE — Binary search: which layer/module is the fault in?\n"
                "  5. FIX — Only change code once the root cause is confirmed\n"
                "\n"
                "  Avoid: guessing fixes, changing multiple things at once, fixing symptoms.\n",
                file=sys.stderr,
                flush=True
            )

    except Exception:
        pass
    finally:
        sys.exit(0)


if __name__ == '__main__':
    main()
