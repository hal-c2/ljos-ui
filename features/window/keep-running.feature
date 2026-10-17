# Sources:
#   https://doc.qt.io/qt-6/qguiapplication.html#quitOnLastWindowClosed-prop
#   https://developer.apple.com/documentation/appkit/nsapplicationdelegate/applicationshouldhandlereopen(_:hasvisiblewindows:)
#   https://github.com/hal-c2/ljos-ui/issues/82
# Qt 6.11.2 (qml6, offscreen) ends the application when its last window
# has closed, and its QML has neither `Qt.application.quitOnLastWindowClosed`
# nor a signal for being opened again: both are Ljós's. That the loop ends
# or goes on is the desktop shell's, which a scenario does not run; these
# hold what the application says and does.
@ljos-only
Feature: Running on without a window
  An application ends when its last window has closed, unless it says that
  it keeps running: then it lives on without a window until `Qt.quit()`,
  and hears when its user opens it again, as a click on its icon in the
  Dock does, so that it can show a window.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        id: w; objectName: "w"; width: 300; height: 200; visible: true
        property bool quits: Qt.application.quitOnLastWindowClosed
        property int reopens: 0
        property bool hadWindow: false
        function keepRunning() { Qt.application.quitOnLastWindowClosed = false }
        function leave() { Qt.quit() }
        Connections {
          target: Qt.application
          function onReopened(hasVisibleWindows) {
            w.reopens++
            w.hadWindow = hasVisibleWindows
            w.show()
          }
        }
      }
      """

  @api-QtQuick.Application.quitOnLastWindowClosed
  Scenario: An application ends with its last window unless it says otherwise
    Then the property "quits" of "w" is true

  @api-QtQuick.Application.quitOnLastWindowClosed
  Scenario: An application can say that it keeps running without a window
    When the method "keepRunning" of "w" is called
    Then the property "quits" of "w" is false

  @api-QtQml.Qt.quit
  Scenario: An application that keeps running still ends when it is asked to
    When the method "keepRunning" of "w" is called
    And the method "close" of "w" is called
    Then the application was not asked to quit
    When the method "leave" of "w" is called
    Then the application was asked to quit

  @api-QtQuick.Application.reopened
  Scenario: Opened again with no window shown, the application shows one
    When the method "keepRunning" of "w" is called
    And the method "close" of "w" is called
    Then the property "visible" of "w" is false
    When the application is opened again
    Then the property "reopens" of "w" is 1
    And the property "hadWindow" of "w" is false
    And the property "visible" of "w" is true

  @api-QtQuick.Application.reopened
  Scenario: Opened again while a window is shown, the application is told so
    When the application is opened again
    Then the property "reopens" of "w" is 1
    And the property "hadWindow" of "w" is true
