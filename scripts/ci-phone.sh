#!/usr/bin/env bash
# The @android scenarios on the emulator ci.yml's `phone` job started, and, when one fails, what
# the phone said, in <dir>: its log, its screen and the scenario application's fault
# (examples/android_scenarios, `fault_file`). Gathered here, while the emulator still runs: the
# job's next step comes after the emulator is shut down.
#
#     scripts/ci-phone.sh <dir>
set -euo pipefail
out="${1:?scripts/ci-phone.sh <dir>}"
mkdir -p "$out"
app=dev.ljos.ljos_example_android_scenarios
adb logcat -c
status=0
mise run features:android features/mobile -t @android || status=$?
if [ "$status" -ne 0 ]; then
  adb logcat -d > "$out/logcat.txt" || true
  adb exec-out screencap -p > "$out/screen.png" || true
  # A debuggable application's files are its own; run-as reads them as it would.
  adb shell run-as "$app" cat files/scenario-fault.txt > "$out/scenario-fault.txt" 2> /dev/null || true
fi
exit "$status"
