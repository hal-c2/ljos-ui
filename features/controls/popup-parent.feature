# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#parent-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A popup has its parent before its bindings run
  A popup declared in an item has that item as its parent from the start,
  so a binding of the popup's own that reads `parent` sees it the first
  time it runs.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 300; height: 300
        property real seen: -1
        Popup {
          objectName: "p"
          width: parent.width / 1.5
          Component.onCompleted: root.seen = width
        }
      }
      """

  @api-QtQuick.Controls.Popup.parent
  Scenario: A size bound to the parent is right when the popup is completed
    Then the property "seen" of "root" is 200
    And the property "parent.objectName" of "p" is "root"

  Scenario: A binding that reads the parent is not run against no parent
    Then the log has no warnings
