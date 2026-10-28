#!/usr/bin/env bash
# One eval run: a fresh project, one model, with or without the skills, then
# the task's graders.
#
#   skills/evals/run.sh <haiku|sonnet|opus> <baseline|skill> <task> [agent|grade]
#
# With no phase it does both. The agent has AGENT_TIMEOUT seconds (540).
# Needs: claude, a release `ljos` (LJOS), the scenario runner built from
# `cargo test -p ljos_bdd --test features --no-run` (LJOS_FEATURES_RUNNER),
# features-lint (LJOS_FEATURES_LINT) and qml6 on PATH for the qt backend.
# Runs land in RUNS (default skills/evals/runs), never in a checkout. Cargo
# runs with RUST_ENV (`KEY=VALUE ...`), which can pin a toolchain and share
# a target directory warmed with the ljos crate.
set -euo pipefail
model=$1 cond=$2 task=$3 phase=${4:-both}
EVALS=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$EVALS/../.." && pwd)
RUNS=${RUNS:-$EVALS/runs}
: "${LJOS:?a release ljos}" "${LJOS_FEATURES_RUNNER:?the scenario runner}" "${LJOS_FEATURES_LINT:?features-lint}"
export LJOS LJOS_FEATURES_RUNNER LJOS_FEATURES_LINT EVALS
declare -A budget=([haiku]=1.5 [sonnet]=4 [opus]=8)

RUN="$RUNS/$model-$cond-$task"
export RUN
project="$RUN/project"
spec="$EVALS/$task.json"
read -ra rust_env <<< "${RUST_ENV:-}"
tools_dir=$(dirname "$LJOS")

grade() {
  env PATH="$tools_dir:$PATH" CARGO_NET_OFFLINE=true "${rust_env[@]}" \
    python3 -I "$EVALS/grade.py" "$spec" "$project" "$RUN/grades.json" || true
}
if [[ $phase == grade ]]; then
  grade
  exit
fi

rm -rf "$RUN"
mkdir -p "$project"
python3 -I - "$spec" "$EVALS" "$project" <<'PY'
import json, os, shutil, sys
spec, evals, project = sys.argv[1:]
for f in json.load(open(spec))["files"]:
    shutil.copy(os.path.join(evals, f), project)
PY
if [[ $task == embed-in-rust ]]; then
  cp "$REPO/Cargo.lock" "$project/"
fi
if [[ $cond == skill ]]; then
  mkdir -p "$project/.claude/skills"
  for s in "$REPO"/skills/*/SKILL.md; do
    cp -r "$(dirname "$s")" "$project/.claude/skills/"
  done
fi
cat > "$RUN/mcp.json" <<JSON
{"mcpServers": {"ljos": {"command": "$LJOS", "args": ["mcp", "--project", "$project", "--caps", "fs,run", "--features-runner", "$LJOS_FEATURES_RUNNER"]}}}
JSON
git -C "$project" init -q

query=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["query"].replace("{{LJOS_CRATE}}", sys.argv[2]))' "$spec" "$REPO/crates/ljos")
mapfile -t tools < <(python3 -I -c 'import json,sys; print("\n".join(json.load(open(sys.argv[1]))["tools"]))' "$spec")

# No display: nothing the agent starts opens a window on this desktop.
(cd "$project" && env -u WAYLAND_DISPLAY -u DISPLAY PATH="$tools_dir:$PATH" CARGO_NET_OFFLINE=true "${rust_env[@]}" \
  timeout "${AGENT_TIMEOUT:-540}" claude -p "$query" --model "$model" \
    --setting-sources project --strict-mcp-config --mcp-config "$RUN/mcp.json" \
    --permission-mode acceptEdits --allowedTools "${tools[@]}" \
    --max-budget-usd "${budget[$model]}" \
    --output-format stream-json --verbose > "$RUN/transcript.jsonl" 2> "$RUN/claude.err") || echo "claude exited $?"

if [[ $phase == both ]]; then
  grade
fi
