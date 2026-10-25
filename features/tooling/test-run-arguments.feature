# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-application.html#arguments-prop
#   https://doc.qt.io/qt-6/qtquicktest-index.html
# Verified on Qt 6.11.2 (qmltestrunner, offscreen). The two runners are
# different programs started with their own options, so the scenario
# checks what both command lines have.

@qt-parity
Feature: What a test reads of the command line
  A test file reads the command line the runner was started with in
  `Application.arguments`, the program first, as an application does.

  @api-QtQuick.Application.arguments
  Scenario: A test reads the runner's command line, the program first
    Given the file "tst_arguments.qml":
      """qml
      import QtQuick
      import QtTest
      TestCase {
        name: "Arguments"
        function test_program_first() {
          verify(Application.arguments.length > 1)
          verify(Application.arguments[0].endsWith(Application.name))
        }
      }
      """
    When the tests are run
    Then the test report is:
      """
      PASS   : Arguments::initTestCase()
      PASS   : Arguments::test_program_first()
      PASS   : Arguments::cleanupTestCase()
      Totals: 3 passed, 0 failed, 0 skipped
      """
