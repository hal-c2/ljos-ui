# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#quit-method
#   https://doc.qt.io/qt-6/qml-qtquick-window.html

# The oracle is itself the application that would end: it cannot say that
# it was asked to.
@ljos-only
Feature: Ending the application
  `Qt.quit()` asks the application to end. Closing a window does not: the
  host ends the application when its last window has closed.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "w"
        width: 300; height: 200
        visible: true
        function leave() { Qt.quit() }
      }
      """

  @api-QtQml.Qt.quit
  Scenario: Nothing ends the application by itself
    Then the application was not asked to quit

  @api-QtQml.Qt.quit
  Scenario: Qt.quit() asks the application to end
    When the method "leave" of "w" is called
    Then the application was asked to quit

  @api-QtQuick.Window.close
  Scenario: Closing a window is not a request to quit
    When the method "close" of "w" is called
    Then the application was not asked to quit
