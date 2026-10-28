"""Summarise eval runs: turns, cost, the tools called, and the grades.

    python3 -I summarise.py <run dir>...
"""

import collections
import json
import pathlib
import sys


def main() -> None:
    for run in map(pathlib.Path, sys.argv[1:]):
        tools = collections.Counter()
        result = {}
        transcript = run / "transcript.jsonl"
        for line in transcript.open() if transcript.exists() else []:
            try:
                message = json.loads(line)
            except ValueError:
                continue
            if message.get("type") == "result":
                result = message
            inner = message.get("message")
            for block in (inner.get("content") if isinstance(inner, dict) else None) or []:
                if isinstance(block, dict) and block.get("type") == "tool_use":
                    name = block["name"].removeprefix("mcp__ljos__")
                    if name == "Bash":
                        name = "Bash:" + block["input"].get("command", "").split()[0]
                    tools[name] += 1
        grades = run / "grades.json"
        graded = json.loads(grades.read_text()) if grades.exists() else {}
        print(f"== {run.name}: {result.get('subtype')} turns={result.get('num_turns')} "
              f"cost={result.get('total_cost_usd', 0):.2f}")
        print("   grades:", " ".join(f"{k}={'pass' if v['pass'] else 'FAIL'}"
                                     for k, v in graded.items()))
        print("   tools:", ", ".join(f"{k}x{n}" for k, n in tools.most_common()))


if __name__ == "__main__":
    main()
