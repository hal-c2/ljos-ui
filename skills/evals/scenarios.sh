#!/usr/bin/env bash
# Grader: the project's scenarios pass on one backend (ljos or qt), and there
# are at least three of them, so an empty or untagged file does not pass.
set -euo pipefail
log="$RUN/scenarios-$1.log"
LJOS_BDD_BACKEND="$1" "$LJOS_FEATURES_RUNNER" --input "$PWD/features/*.feature" | tee "$log"
python3 -I - "$log" <<'PY'
import re, sys
summary = re.search(r"(\d+) scenarios? \((.*)\)", open(sys.argv[1]).read())
if not summary:
    sys.exit("no scenario ran")
total = int(summary.group(1))
passed = re.search(r"(\d+) passed", summary.group(2))
if total < 3 or not passed or int(passed.group(1)) != total:
    sys.exit(f"{summary.group(0)}: want at least 3, all passed")
PY
