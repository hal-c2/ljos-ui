# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#parent-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-overlay.html
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity
Feature: A popup whose parent moves to another window
  A popup is shown in the overlay of the window its parent item is in.
  When the parent moves into another window, the popup goes with it: it
  opens there, and an open popup moves over at once.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      import QtQuick.Window
      Item {
        id: root
        objectName: "root"
        width: 200; height: 200
        property string at: inner.Window.window === other ? "other"
                          : inner.Window.window === root.Window.window ? "first" : "none"
        Item {
          id: holder
          width: 50; height: 50
          Popup {
            id: p
            objectName: "p"
            width: 30; height: 30
            Item { id: inner; anchors.fill: parent }
          }
        }
        Window { id: other; width: 300; height: 300; visible: true }
        function moveOver() { holder.parent = other.contentItem }
        function moveBack() { holder.parent = root }
      }
      """

  Scenario: A popup opened in the first window is shown there
    When the method "open" of "p" is called
    And 50 milliseconds pass
    Then the property "at" of "root" is "first"

  Scenario: A popup whose parent moved opens in the other window
    When the method "moveOver" of "root" is called
    And the method "open" of "p" is called
    And 50 milliseconds pass
    Then the property "at" of "root" is "other"
    And the property "visible" of "p" is true

  Scenario: An open popup moves with its parent
    When the method "open" of "p" is called
    And 50 milliseconds pass
    And the method "moveOver" of "root" is called
    And 50 milliseconds pass
    Then the property "at" of "root" is "other"
    When the method "moveBack" of "root" is called
    And 50 milliseconds pass
    Then the property "at" of "root" is "first"
    And the property "opened" of "p" is true
