# Sources:
#   docs/ARCHITECTURE.md section 18 (behaviour first, Qt as the oracle)
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
# `ljos-fuzz` is Ljós's own tool: Qt has nothing to compare it with.
# The planted oracle is Ljós again, with every observed `int` one higher, so a run
# shows a known difference without Qt. The real oracle is `qml6` (`mise run fuzz:qt`).

@ljos-only @headless
Feature: Comparing Ljós with Qt on random programs
  `ljos-fuzz` writes random QML programs that Qt loads, runs each one on
  Ljós and on the oracle, and compares what both show. A difference is
  shrunk to a small program and put in a bucket named by what the program
  does. A bucket on the known list is reported but does not fail the run;
  a new one does, and its program is written out to be looked at.

  Scenario: A known difference is shrunk and reported as known
    Given the file "known/bindings.txt":
      """
      # signature  reason
      value:load:number  planted: the oracle adds one to every int
      """
    When the author runs "ljos-fuzz bindings --oracle planted --cases 12 --seed 7 --jobs 2 --known known --out out"
    Then the command exits with status 0
    And the command's output mentions "seed 7, 12 cases"
    And the command's output mentions "known value:load:number  (1 object, 1 property, 0 steps;"
    And the command's output mentions "out/value_load_number"
    And the command's output does not mention "NEW"

  Scenario: A difference not on the known list fails the run
    Given the file "known/bindings.txt":
      """
      # nothing is known yet
      """
    When the author runs "ljos-fuzz bindings --oracle planted --cases 12 --seed 7 --jobs 2 --known known --out out"
    Then the command exits with status 1
    And the command's output mentions "NEW   value:load:number"
