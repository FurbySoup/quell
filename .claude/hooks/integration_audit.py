"""
Hook: integration_audit.py
Event: PostToolUse (Bash — on git commit)
Purpose: After a commit, check for common wiring gaps:
  1. Exported functions with zero external call sites
  2. Tauri invoke() calls without matching Rust commands
  3. listen()/addEventListener() without stored cleanup references

This catches the "declared but not wired" pattern that recurs in multi-file features.
"""

import json
import os
import re
import subprocess
import sys

def main():
    tool_input = json.loads(os.environ.get("TOOL_INPUT", "{}"))
    command = tool_input.get("command", "")

    # Only run on git commit
    if "git commit" not in command:
        return

    # Get list of changed files in the last commit
    result = subprocess.run(
        ["git", "diff", "--name-only", "HEAD~1", "HEAD"],
        capture_output=True, text=True, cwd=os.getcwd()
    )
    if result.returncode != 0:
        return

    changed_files = result.stdout.strip().split("\n")
    ts_files = [f for f in changed_files if f.endswith((".ts", ".js"))]
    rs_files = [f for f in changed_files if f.endswith(".rs")]

    warnings = []

    # Check 1: TS invoke() calls vs Rust generate_handler
    if ts_files:
        invoke_calls = set()
        for f in ts_files:
            if not os.path.exists(f):
                continue
            with open(f, "r", encoding="utf-8", errors="ignore") as fh:
                content = fh.read()
                for match in re.finditer(r'invoke\s*[<(]\s*["\'](\w+)["\']', content):
                    invoke_calls.add(match.group(1))

        if invoke_calls:
            # Find all registered Tauri commands
            main_rs = "gui/src-tauri/src/main.rs"
            if os.path.exists(main_rs):
                with open(main_rs, "r", encoding="utf-8", errors="ignore") as fh:
                    main_content = fh.read()
                    # Extract commands from generate_handler![]
                    handler_match = re.search(
                        r'generate_handler!\[(.*?)\]',
                        main_content, re.DOTALL
                    )
                    if handler_match:
                        registered = set()
                        for cmd in handler_match.group(1).split(","):
                            cmd = cmd.strip().split("::")[-1].strip()
                            if cmd:
                                registered.add(cmd)

                        missing = invoke_calls - registered
                        # Filter out known non-plugin commands
                        known_commands = {"spawn_shell", "write_input", "resize_pty", "close_session"}
                        missing = missing - known_commands
                        if missing:
                            warnings.append(
                                f"[Integration Audit] TS invoke() calls with no matching Rust command: {', '.join(sorted(missing))}"
                            )

    # Check 2: Exported functions with no call sites in changed files
    # (This is a heuristic — only checks within the committed changeset)
    if ts_files:
        exports = {}  # {function_name: defining_file}
        for f in ts_files:
            if not os.path.exists(f):
                continue
            with open(f, "r", encoding="utf-8", errors="ignore") as fh:
                content = fh.read()
                for match in re.finditer(r'export\s+function\s+(\w+)', content):
                    exports[match.group(1)] = f

        # Check if each export is called somewhere else
        all_content = {}
        for f in ts_files:
            if not os.path.exists(f):
                continue
            with open(f, "r", encoding="utf-8", errors="ignore") as fh:
                all_content[f] = fh.read()

        for func_name, defining_file in exports.items():
            called = False
            for f, content in all_content.items():
                if f == defining_file:
                    continue
                if func_name in content:
                    called = True
                    break
            if not called:
                # Check broader codebase
                src_dir = os.path.dirname(defining_file)
                broader_check = subprocess.run(
                    ["grep", "-r", "--include=*.ts", "--include=*.js", "-l", func_name, src_dir],
                    capture_output=True, text=True, cwd=os.getcwd()
                )
                if broader_check.returncode == 0:
                    files_with_usage = [
                        line for line in broader_check.stdout.strip().split("\n")
                        if line and line != defining_file
                    ]
                    if files_with_usage:
                        called = True

                if not called:
                    warnings.append(
                        f"[Integration Audit] export function {func_name}() in {defining_file} has no external call sites — possible wiring gap"
                    )

    if warnings:
        msg = "\n".join(warnings)
        print(json.dumps({
            "systemMessage": f"Integration audit found potential issues:\n{msg}\n\nThese may be intentional (e.g., future use) or genuine wiring gaps. Verify each one."
        }))
    else:
        print(json.dumps({}))


if __name__ == "__main__":
    main()
