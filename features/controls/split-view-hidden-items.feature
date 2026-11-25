# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-splitview.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: SplitView items that are hidden
  An item of a split view that is not visible takes no room: the items
  that are shown share the view as if it were not there, and the handle
  that follows it is hidden. So is the handle after the last item shown,
  which has nothing after it to separate. Shown again, the item takes back
  the room it prefers.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property string seen: ""
        SplitView {
          id: sv; objectName: "sv"; anchors.fill: parent
          Rectangle { id: a; objectName: "a"; implicitWidth: 100; SplitView.minimumWidth: 50 }
          Rectangle { id: b; objectName: "b"; implicitWidth: 80; SplitView.fillWidth: true }
          Rectangle { id: c; objectName: "c"; SplitView.preferredWidth: 70; SplitView.maximumWidth: 120 }
        }
        function shown() { seen = sv.children.filter(h => h !== sv.contentItem).map(h => h.visible).join(" ") }
        function preferC(w) { c.SplitView.preferredWidth = w }
        function vertical() { sv.orientation = Qt.Vertical; b.SplitView.fillHeight = true }
      }
      """

  Scenario: A hidden last item leaves its room to the item that fills
    When 100 milliseconds pass
    And the property "visible" of "c" is set to false
    And 100 milliseconds pass
    And the method "shown" of "root" is called
    Then the property "x" of "b" is 106
    And the property "width" of "b" is 294
    And the property "seen" of "root" is "true false"

  Scenario: A hidden first item leaves its room to the item that fills
    When 100 milliseconds pass
    And the property "visible" of "a" is set to false
    And 100 milliseconds pass
    And the method "shown" of "root" is called
    Then the property "x" of "b" is 0
    And the property "width" of "b" is 324
    And the property "x" of "c" is 330
    And the property "seen" of "root" is "false true"

  Scenario: When the item that fills is hidden the last item shown fills, within its maximum
    When 100 milliseconds pass
    And the property "visible" of "b" is set to false
    And 100 milliseconds pass
    And the method "shown" of "root" is called
    Then the property "x" of "c" is 106
    And the property "width" of "c" is 120
    And the property "seen" of "root" is "true false"

  Scenario: An item shown again takes back the room it prefers
    When 100 milliseconds pass
    And the method "preferC" of "root" is called with 90
    And the property "visible" of "c" is set to false
    And 100 milliseconds pass
    And the property "visible" of "c" is set to true
    And 100 milliseconds pass
    And the method "shown" of "root" is called
    Then the property "x" of "c" is 310
    And the property "width" of "c" is 90
    And the property "width" of "b" is 198
    And the property "seen" of "root" is "true true"

  Scenario: A view hidden and shown again keeps hidden what was hidden before
    When 100 milliseconds pass
    And the property "visible" of "c" is set to false
    And 100 milliseconds pass
    And the property "visible" of "sv" is set to false
    And 100 milliseconds pass
    And the property "visible" of "sv" is set to true
    And 100 milliseconds pass
    And the method "shown" of "root" is called
    Then the property "width" of "b" is 294
    And the property "visible" of "c" is false
    And the property "seen" of "root" is "true false"

  Scenario: A hidden item of a vertical split view leaves its height to the item that fills
    When 100 milliseconds pass
    And the method "vertical" of "root" is called
    And the property "visible" of "c" is set to false
    And 100 milliseconds pass
    Then the property "y" of "b" is 6
    And the property "height" of "b" is 294
