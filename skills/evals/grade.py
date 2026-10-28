"""Run a task's graders in a run's project and record pass or fail.

    python3 -I grade.py <task.json> <project> <results.json>

Each grader is a shell command run in the project; it passes when it exits 0.
The environment names the tools: LJOS, LJOS_FEATURES_RUNNER,
LJOS_FEATURES_LINT, EVALS and RUN (where a grader may leave its logs).
"""

import json
import subprocess
import sys


def main() -> int:
    task_file, project, results = sys.argv[1:]
    task = json.load(open(task_file))
    grades = {}
    for grader in task["graders"]:
        try:
            done = subprocess.run(["bash", "-c", grader["run"]], cwd=project,
                                  capture_output=True, text=True, timeout=1800)
            code, output = done.returncode, done.stdout + done.stderr
        except subprocess.TimeoutExpired:
            code, output = 124, "timed out"
        grades[grader["name"]] = {"pass": code == 0, "exit": code,
                                  "tail": output[-3000:]}
        print(f"{grader['name']}: {'pass' if code == 0 else 'FAIL'} ({code})")
    json.dump(grades, open(results, "w"), indent=1)
    return 0 if all(g["pass"] for g in grades.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
