# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#quit-method

# The oracle is itself the application that would end, and that it ends is
# what is looked at: the process, not a request noted on the way.
@ljos-only @desktop
Feature: A window at rest ends when it is asked to
  A window that has nothing to draw sleeps. What wakes it may be the last
  thing it does: when that asks the application to end, it ends, and does
  not go back to sleep with the request noted.

  Scenario: Qt.quit() from the only timer of a window at rest ends the application
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        Timer { interval: 3000; running: true; onTriggered: Qt.quit() }
      }
      """
    Then the application has ended within 8000 milliseconds
