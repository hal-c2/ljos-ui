# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#parent-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#anchors.centerIn-prop
# Verified on Qt 6.11.2 with a stand-alone qml6 run (x 100, y 75, width 200,
# height 150). The oracle's driver makes the window with `createObject` under
# its own window and then puts a centred popup at minus half its size, so these
# scenarios run on Ljós only.

@ljos-only
Feature: A popup declared in an application window
  A popup's parent is the content item of the window it is declared in:
  its size can follow `parent.width` and it can be centred in it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        width: 400; height: 300
        visible: true
        Popup {
          objectName: "p"
          padding: 20
          anchors.centerIn: parent
          width: parent.width / 2
          height: parent.height / 2
        }
      }
      """

  @api-QtQuick.Controls.Popup.parent
  Scenario: A size bound to the parent follows the window's content item
    Then the property "width" of "p" is 200
    And the property "height" of "p" is 150

  @api-QtQuick.Controls.Popup.anchors.centerIn
  Scenario: A popup centred in the window's content item
    When the method "open" of "p" is called
    Then the property "x" of "p" is 100
    And the property "y" of "p" is 75
    And the property "contentItem.parent.x" of "p" is 100

  Scenario: A binding that reads the parent is not run against no parent
    Then the log has no warnings
