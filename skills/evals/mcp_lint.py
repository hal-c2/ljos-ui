"""Grader: `qml_lint` of the authoring server reports no Error.

    python3 -I mcp_lint.py <ljos> <project> [paths...]

Starts `ljos mcp --project <project>` on stdio, calls `qml_lint` on the
paths (the whole project if none), prints every finding, and exits 1 when
one is an Error, 2 when the call itself fails.
"""

import json
import subprocess
import sys


def main() -> int:
    ljos, project, *paths = sys.argv[1:]
    server = subprocess.Popen(
        [ljos, "mcp", "--project", project],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
    )

    def send(message: dict) -> None:
        server.stdin.write(json.dumps(message) + "\n")
        server.stdin.flush()

    def answer(want: int) -> dict:
        for line in server.stdout:
            message = json.loads(line)
            if message.get("id") == want:
                return message
        raise SystemExit("the server ended before it answered")

    send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-11-25", "capabilities": {},
        "clientInfo": {"name": "skills-eval", "version": "1"}}})
    answer(1)
    send({"jsonrpc": "2.0", "method": "notifications/initialized"})
    findings = []
    cursor = None
    call = 2
    while True:
        args = {"paths": paths or ["."]}
        if cursor:
            args["cursor"] = cursor
        send({"jsonrpc": "2.0", "id": call, "method": "tools/call",
              "params": {"name": "qml_lint", "arguments": args}})
        reply = answer(call)
        call += 1
        result = reply.get("result") or {}
        if "error" in reply or result.get("isError"):
            print(json.dumps(reply, indent=1))
            return 2
        out = result.get("structuredContent") or json.loads(result["content"][0]["text"])
        findings += out.get("diagnostics", [])
        cursor = out.get("next_cursor")
        if not cursor:
            break
    server.stdin.close()
    server.wait(timeout=30)
    errors = 0
    for d in findings:
        severity = str(d.get("severity", "")).lower()
        errors += severity == "error"
        print(f"{d.get('file')}:{d.get('line')}: {severity}: {d.get('message')} [{d.get('code')}]")
    print(f"qml_lint: {len(findings)} findings, {errors} errors")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
