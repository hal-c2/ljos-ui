# Sources:
#   docs/ARCHITECTURE.md section 8.5 (memory budgets)
#   https://docs.kernel.org/admin-guide/cgroup-v2.html (memory.max, /proc/self/cgroup)
#   https://man7.org/linux/man-pages/man5/proc_meminfo.5.html (MemTotal)
# Qt has no budget of the kind: its pixmap cache is a fixed size it does not tell QML.
# The system files are given under a directory of the scenario's own, as `/` would hold them.

@ljos-only @headless
Feature: Decoded pictures fit the memory the application may use
  Pictures that are decoded and not shown are kept, least recently used
  first, up to a budget. On a desktop the budget is an eighth of the
  memory the application may use, and never more than 256 MiB; on a phone
  it is 6 % of that memory, and never more than 64 MiB. The memory the
  application may use is the machine's, or less when the application runs
  under a limit (a container, a systemd unit or scope).

  Background:
    Given the file "root/proc/self/cgroup":
      """text
      0::/user.slice/app.scope
      """

  Scenario: A large desktop keeps up to 256 MiB of pictures
    Given the file "root/proc/meminfo":
      """text
      MemTotal:       16777216 kB
      MemFree:         8388608 kB
      """
    Then a desktop with the system files under "root" keeps up to 256 MiB of decoded pictures

  Scenario: A small machine keeps an eighth of its memory
    Given the file "root/proc/meminfo":
      """text
      MemTotal:         524288 kB
      """
    Then a desktop with the system files under "root" keeps up to 64 MiB of decoded pictures
    And a phone with the system files under "root" keeps up to 30.72 MiB of decoded pictures

  Scenario: A limit on the application is the memory it may use
    Given the file "root/proc/meminfo":
      """text
      MemTotal:       16777216 kB
      """
    And the file "root/sys/fs/cgroup/user.slice/app.scope/memory.max":
      """text
      268435456
      """
    Then a desktop with the system files under "root" keeps up to 32 MiB of decoded pictures

  Scenario: A limit on a group the application is in counts too
    Given the file "root/proc/meminfo":
      """text
      MemTotal:       16777216 kB
      """
    And the file "root/sys/fs/cgroup/user.slice/memory.max":
      """text
      536870912
      """
    And the file "root/sys/fs/cgroup/user.slice/app.scope/memory.max":
      """text
      max
      """
    Then a desktop with the system files under "root" keeps up to 64 MiB of decoded pictures

  Scenario: A machine that does not say its memory keeps up to 256 MiB
    Then a desktop with the system files under "root" keeps up to 256 MiB of decoded pictures
