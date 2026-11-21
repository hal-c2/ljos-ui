# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-timer.html
#   docs/ARCHITECTURE.md principle 6 (work is scheduled by timers, nothing polls)
# Measured on Qt 6.12.0 (qml6, offscreen): a repeating Timer of 10 ms whose
# handler busies the thread for 80 ms triggers 13 times in a second, once each
# time the handler returns, with a `Qt.callLater` queued by one run done before
# the next; the application quits at its 1000 ms single shot after 1.2 s. A
# 10 ms Timer held up 105 ms by another handler triggers once, late, and then
# again on its own phase.

# The handler's time has to pass on the real clock, which only a running
# application has, so the oracle is the process ending, as in
# window/quit-at-rest.feature.
@ljos-only @desktop
Feature: A Timer whose handler outlasts its interval
  A repeating Timer that comes due late triggers once, not once for every
  interval it missed, and then keeps to its own phase. A handler that takes
  longer than the interval slows the timer down to one run a turn; it never
  makes a turn of the application longer, so input and other timers are
  still answered.

  Scenario: A slow repeating handler does not hold up a single shot that quits
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        Timer {
          interval: 10; repeat: true; running: true
          onTriggered: { var t = Date.now(); while (Date.now() - t < 80) {} }
        }
        Timer { interval: 1000; running: true; onTriggered: Qt.quit() }
      }
      """
    Then the application has ended within 5000 milliseconds
