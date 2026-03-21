"""UserPromptSubmit hook — injects live-proving context during feature planning.

Fires when the user submits a prompt. If the prompt mentions planning,
implementing, or starting a feature, injects a reminder to consider
live-proving steps as part of the plan.

Non-blocking — exits 0, returns additionalContext via stdout JSON.
"""
import json
import re
import sys

# Patterns suggesting feature planning/implementation is starting
PLANNING_PATTERNS = [
    r'\b(?:plan|implement|build|add|create|start)\b.*\b(?:feature|milestone|component)\b',
    r'\bmilestone\s+\d',
    r'\bphase\s+\d',
    r'\benter\s+plan\b',
    r'\b(?:design|architect|scaffold)\b',
    r'\bnew\s+(?:module|crate|component)\b',
]

# Rate limit: only inject once per prompt containing these patterns
def _is_planning_prompt(prompt: str) -> bool:
    prompt_lower = prompt.lower()
    for pattern in PLANNING_PATTERNS:
        if re.search(pattern, prompt_lower):
            return True
    return False


def main():
    try:
        try:
            hook_input = json.loads(sys.stdin.read())
        except (json.JSONDecodeError, ValueError):
            return

        prompt = hook_input.get('prompt', '')
        if not prompt or not _is_planning_prompt(prompt):
            return

        # Return additionalContext via stdout JSON (shown to Claude)
        output = {
            "hookSpecificOutput": {
                "hookEventName": "UserPromptSubmit",
                "additionalContext": (
                    "[live-proving-planning] This looks like feature planning. "
                    "Include in your plan: (1) Which live-proving steps are automated "
                    "(Tier 1 replay tests)? (2) Which are automatable but not yet "
                    "(Tier 2 scroll probe)? (3) Which need human interaction with "
                    "the terminal/GUI (Tier 3 visual walkthrough)? "
                    "Document these in the plan before implementation begins."
                )
            }
        }
        print(json.dumps(output), flush=True)

    except Exception:
        pass
    finally:
        sys.exit(0)


if __name__ == '__main__':
    main()
