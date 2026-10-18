# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Change handlers that write their own property
  A handler of a property's change may write the property again; Qt runs
  the handler again, inside the first, until it stops writing a new
  value. Qt stops a handler that never does when its script stack runs
  out (835 runs deep on Qt 6.11.2), with a warning.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int n: 0
        property int u: 0
        onNChanged: if (n < 800) n = n + 1
        onUChanged: u = u + 1
      }
      """

  @qt-parity
  Scenario: A handler runs again for as long as it writes a new value
    When the property "n" of "root" is set to 1
    Then the property "n" of "root" is 800
    And the log has no warnings

  # Qt stops where its script stack runs out, which depends on the
  # handler; Ljós stops after a fixed number of runs, above Qt's depth, and
  # names the property whose handlers it stopped.
  @qt-diverges
  Scenario: A handler that never stops writing is stopped, and said to be
    When the property "u" of "root" is set to 1
    Then the log has a warning matching "QML \w+: property .u. handlers re-ran 1000 times in one update"
