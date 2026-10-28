#!/usr/bin/env bash
# Grader: the ported files still draw and react as the original does. The
# scenarios in fixtures/port-from-qtquick/behaviour.feature.in pass on the
# original under qml6; here they run on Ljós with the ported files inlined.
set -euo pipefail
out="$RUN/behaviour"
mkdir -p "$out"
python3 -I - "$EVALS/fixtures/port-from-qtquick/behaviour.feature.in" "$out/behaviour.feature" <<'PY'
import sys
template, target = sys.argv[1:]
text = open(template).read()
for name in ("Button.qml", "application.qml"):
    body = "\n".join(("        " + line).rstrip() for line in open(name).read().splitlines())
    text = text.replace("{{" + name + "}}", body)
open(target, "w").write(text)
PY
LJOS_BDD_BACKEND=ljos "$LJOS_FEATURES_RUNNER" --input "$out/*.feature" | tee "$out/run.log"
grep -q '3 scenarios (3 passed)' "$out/run.log"
