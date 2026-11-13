# Sources:
#   docs/ARCHITECTURE.md sections 2 (0 % CPU when idle), 8.5 (memory budgets) and 18.6 (`bench`)
#   https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html
#   https://docs.kernel.org/admin-guide/cgroup-v2.html (memory.peak, memory.events, cpu.stat, cpu.max)
# `ljos bench` is Ljós's own program: Qt has nothing to compare it with.
# The launcher here stands in for `systemd-run --user --scope`: it writes the files a scope's
# cgroup would have under the directory given as `--cgroup-root`, then starts what it was given.
# The application stands in for a runtime and only waits. Nothing draws, so no first frame is
# seen and the report says so. The real launcher and real runtimes are what `mise run bench`
# uses; their numbers are not part of any scenario.

@ljos-only @headless
Feature: Measuring an application under a limit
  `ljos bench` starts an application inside a limit on memory and CPU and
  reports what it used as JSON: its peak and settled memory from the
  limit's own accounting, the CPU it took, and whether the kernel killed it
  for want of memory. Under load it is clicked, dragged, scrolled, typed
  into or resized, as asked. On a compositor it times the first frame shown and the first
  frame drawn in answer to the compositor once the window is up. It never measures an application that is not inside the limit it
  asked for. The same application written for another toolkit is measured
  beside it the same way, and a report can be compared with an earlier one.

  Background:
    Given the file "Main.qml":
      """qml
      import QtQuick
      Rectangle { width: 100; height: 100 }
      """
    And the file "app.sh":
      """sh
      #!/bin/sh
      exec sleep 30
      """
    And the file "scope.sh":
      """sh
      #!/bin/sh
      # The cgroup the kernel says this process is in, under the root the bench was given.
      d="cg$(cut -d: -f3 /proc/self/cgroup)"
      mkdir -p "$d"
      echo $$ > "$d/cgroup.procs"
      echo "${memory_max:-max}" > "$d/memory.max"
      echo 0 > "$d/memory.swap.max"
      echo "2000 10000" > "$d/cpu.max"
      echo 0-3 > "$d/cpuset.cpus.effective"
      echo 3145728 > "$d/memory.peak"
      echo 2097152 > "$d/memory.current"
      echo "oom 0" > "$d/memory.events"
      echo "oom_kill 0" >> "$d/memory.events"
      echo "usage_usec 25000" > "$d/cpu.stat"
      echo "anon 1048576" > "$d/memory.stat"
      echo "file 1048576" >> "$d/memory.stat"
      """
    And the file "launcher.sh":
      """sh
      #!/bin/sh
      # Everything up to the command is the scope's settings.
      while [ "$1" != "--" ]; do
        echo "$1" >> launcher.args
        case $1 in --property=MemoryMax=*) memory_max=${1##*=} ;; esac
        shift
      done
      shift
      . ./scope.sh
      exec "$@"
      """

  Rule: What the application used is reported as JSON

    Scenario: The limit's accounting is the report
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --memory-max 64M --cpus 4 --cpu-list 0-3 --cpu-factor 20 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      Then the command exits with status 0
      And the JSON file "out/report.json" has "/runs/0/runtime" equal to "ljos"
      And the JSON file "out/report.json" has "/runs/0/profile" equal to "pi-zero"
      And the JSON file "out/report.json" has "/runs/0/limits/memory_max" equal to 67108864
      And the JSON file "out/report.json" has "/runs/0/peak_memory" equal to 3145728
      And the JSON file "out/report.json" has "/runs/0/settled/memory_current" equal to 2097152
      And the JSON file "out/report.json" has "/runs/0/settled/anon" equal to 1048576
      And the JSON file "out/report.json" has "/runs/0/oom_killed" equal to false
      And the JSON file "out/report.json" has "/runs/0/first_frame_ms" equal to null
      And the JSON file "out/report.json" has "/runs/0/input_ready_ms" equal to null
      And the JSON file "out/report.json" has "/runs/0/outcome" equal to "no-frame"
      And the JSON file "out/report.json" has "/runs/0/gesture" equal to "click"

    Scenario Outline: The application is driven with <gesture> while it is measured under load
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --memory-max 64M --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --gesture <gesture> --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      Then the command exits with status 0
      And the JSON file "out/report.json" has "/runs/0/gesture" equal to "<gesture>"

      Examples:
        | gesture |
        | click   |
        | drag    |
        | scroll  |
        | type    |
        | resize  |

    Scenario: The limit is asked for in the scope's own settings
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --memory-max 64M --cpus 4 --cpu-list 0-3 --cpu-factor 20 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      Then the command exits with status 0
      And the file "launcher.args" is:
        """text
        --user
        --scope
        --quiet
        --property=MemoryMax=67108864
        --property=MemorySwapMax=0
        --property=AllowedCPUs=0-3
        --property=CPUQuota=20%
        --property=CPUQuotaPeriodSec=10ms
        """

    Scenario: A pi-zero core is a twentieth of one of this machine's by default
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      Then the command exits with status 0
      And the JSON file "out/report.json" has "/runs/0/limits/cpu_quota_percent" equal to 20
      And the JSON file "out/report.json" has "/runs/0/limits/memory_max" equal to 268435456

  Rule: The work of each thread is counted in instructions where the processor counts them

    # CPU time grows with what else the machine runs; the instructions a
    # thread retires do not. Whether a machine has a counter a user may
    # read is its own (a virtual machine often has none), so what is
    # checked here is that a count not made says why.
    Scenario: Instructions not asked for are not counted, and the report says so
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 1 --no-instructions --startup-timeout 1 --out out"
      Then the command exits with status 0
      And the JSON file "out/report.json" has "/runs/0/load/instructions_missing" equal to "not asked for"

  Rule: What each 3D frame cost is summed up when asked for

    # A runtime that counts its 3D frames writes a line of JSON for each
    # one to the file the bench names, stamped in microseconds since the
    # epoch. This one says it drew twelve times in every frame.
    Scenario: The frames of the run and of its load are summed up
      Given the file "app.sh":
        """sh
        #!/bin/sh
        n=0
        while [ $n -lt 300 ]; do
          n=$((n + 1))
          echo "{\"t_us\":$(date +%s%6N),\"frame\":$n,\"draw_calls\":12,\"encode_ms\":0.5}" >> "$LJOS_3D_FRAMES"
          sleep 0.05
        done
        """
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 1 --frames3d --startup-timeout 1 --out out"
      Then the command exits with status 0
      And the JSON file "out/report.json" has "/runs/0/frames3d/first/draw_calls" equal to 12
      And the JSON file "out/report.json" has "/runs/0/frames3d/first/cpu_ms" equal to 0.5
      And the JSON file "out/report.json" has "/runs/0/frames3d/load/costs/draw_calls/p99" equal to 12

  Rule: A kill for want of memory is a result, not an error

    Scenario: The kernel kills the application
      Given the file "app.sh":
        """sh
        #!/bin/sh
        d="cg$(cut -d: -f3 /proc/self/cgroup)"
        echo "oom 1" > "$d/memory.events"
        echo "oom_kill 1" >> "$d/memory.events"
        kill -9 $$
        """
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --memory-max 64M --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 0 --startup-timeout 5 --out out"
      Then the command exits with status 0
      And the JSON file "out/report.json" has "/runs/0/oom_killed" equal to true
      And the JSON file "out/report.json" has "/runs/0/outcome" equal to "oom-killed"

  Rule: Nothing is measured outside a limit

    Scenario: A launcher that gives no limit is refused
      Given the file "launcher.sh":
        """sh
        #!/bin/sh
        while [ "$1" != "--" ]; do shift; done
        shift
        exec "$@"
        """
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      Then the command exits with status 2
      And the command's output mentions "is not inside the limit"
      And the command's output does not mention "report.json"

    Scenario: A limit other than the one asked for is refused
      Given the file "launcher.sh":
        """sh
        #!/bin/sh
        while [ "$1" != "--" ]; do shift; done
        shift
        memory_max=67108864
        . ./scope.sh
        exec "$@"
        """
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --memory-max 32M --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      Then the command exits with status 2
      And the command's output mentions "memory.max is 67108864, not 33554432"

  Rule: Nothing is measured with motion reduced

    # Reduced motion ends what would move at once and shortens fades, so an
    # application under it does less work than its users see it do. A
    # number measured so would flatter the runtime; the bench says what the
    # desktop's settings portal says of motion and will not run when it is
    # reduced. A desktop with no portal reduces nothing.
    Scenario: The report says how motion was set
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      Then the command exits with status 0
      And the JSON file "out/report.json" has "/motion/reduced_motion" equal to false
      And the JSON file "out/report.json" has "/motion/portal" equal to false

    Scenario: A desktop that reduces motion is refused
      Given the author's desktop setting "org.gnome.desktop.a11y.interface" "reduced-motion" is "reduce"
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      Then the command exits with status 2
      And the command's output mentions "reduced motion is on"
      And the command's output does not mention "report.json"

    Scenario: A desktop whose motion is not reduced is measured
      Given the author's desktop setting "org.freedesktop.appearance" "reduced-motion" is uint32 0
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      Then the command exits with status 0
      And the JSON file "out/report.json" has "/motion/reduced_motion" equal to false
      And the JSON file "out/report.json" has "/motion/portal" equal to true

    Scenario: Another harness asks the bench before it measures
      Given the author's desktop setting "org.freedesktop.appearance" "reduced-motion" is uint32 1
      When the author runs "ljos bench --check-motion"
      Then the command exits with status 2
      And the command's output mentions "reduced motion is on"

  Rule: The same application written for another toolkit is measured beside it

    # A manifest's application may give a command for another runtime: the
    # same window built with another toolkit, measured under the same limit
    # and driven by the same input as `ljos run` and `qml6`.
    Scenario: A runtime is the command an application gives for it
      Given the file "apps.json":
        """json
        {"apps": [{"id": "main", "file": "Main.qml", "commands": {"plain": ["./app.sh"]}}]}
        """
      When the author runs "ljos bench --manifest apps.json --runtime plain --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      Then the command exits with status 0
      And the JSON file "out/report.json" has "/runs/0/app" equal to "main"
      And the JSON file "out/report.json" has "/runs/0/runtime" equal to "plain"
      And the JSON file "out/report.json" has "/runs/0/settled/memory_current" equal to 2097152

    Scenario: A runtime no application gives a command for is refused
      When the author runs "ljos bench Main.qml --runtime plain --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      Then the command exits with status 2
      And the command's output mentions "no runtime \"plain\""

    # A corpus example qml6 cannot load on this machine (a module it lacks)
    # is still measured on the runtimes that can run it.
    Scenario: An application Qt cannot run is measured without Qt
      Given the file "apps.json":
        """json
        {"apps": [{"id": "main", "file": "Main.qml", "qt": false}, {"id": "other", "file": "Main.qml"}]}
        """
      When the author runs "ljos bench --manifest apps.json --runtime ljos,qt --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --qml6 app.sh --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      Then the command exits with status 0
      And the JSON file "out/report.json" has "/runs/0/app" equal to "main"
      And the JSON file "out/report.json" has "/runs/0/runtime" equal to "ljos"
      And the JSON file "out/report.json" has "/runs/1/app" equal to "other"
      And the JSON file "out/report.json" has "/runs/2/runtime" equal to "qt"

  Rule: A report compared with an earlier one says what moved by more than runs of one build do

    # The margins are the noise docs/BENCHMARKS.md describes: memory may
    # move by 10 % and 2 MiB between runs of one build before it counts.
    Scenario: Memory that grew past the margin is worse
      Given the author has run "ljos bench Main.qml --runtime ljos --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 0 --startup-timeout 1 --out before"
      And the file "scope.sh":
        """sh
        #!/bin/sh
        d="cg$(cut -d: -f3 /proc/self/cgroup)"
        mkdir -p "$d"
        echo $$ > "$d/cgroup.procs"
        echo "${memory_max:-max}" > "$d/memory.max"
        echo 0 > "$d/memory.swap.max"
        echo "2000 10000" > "$d/cpu.max"
        echo 0-3 > "$d/cpuset.cpus.effective"
        echo 9437184 > "$d/memory.peak"
        echo 8388608 > "$d/memory.current"
        echo "oom 0" > "$d/memory.events"
        echo "oom_kill 0" >> "$d/memory.events"
        echo "usage_usec 25000" > "$d/cpu.stat"
        echo "anon 1048576" > "$d/memory.stat"
        echo "file 1048576" >> "$d/memory.stat"
        """
      When the author runs "ljos bench Main.qml --runtime ljos --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 0 --startup-timeout 1 --compare before/report.json --out out"
      Then the command exits with status 0
      And the command's output mentions "1 cells worse"
      And the command's output mentions "settled memory 2.00 → 8.00 MiB (margin 2.20)"
      And the JSON file "out/compare.json" has "/cells/0/metric" equal to "settled memory"
      And the JSON file "out/compare.json" has "/cells/0/verdict" equal to "worse"

    Scenario: A report already made is compared without measuring again
      Given the author has run "ljos bench Main.qml --runtime ljos --profile pi-zero --cpus 4 --cpu-list 0-3 --launcher launcher.sh --cgroup-root cg --ljos app.sh --settle 0 --idle 0 --load 0 --startup-timeout 1 --out out"
      When the author runs "ljos bench --compare out/report.json --out out"
      Then the command exits with status 0
      And the command's output mentions "0 cells worse, 0 better, 0 not measured again"
      And the JSON file "out/compare.json" has "/cells/0/verdict" equal to "same"
