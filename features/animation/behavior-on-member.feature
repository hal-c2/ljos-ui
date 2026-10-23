# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qml-qtquick-layouts-layout.html#leftMargin-attached-prop
#   https://doc.qt.io/qt-6/qml-qtquick-rectangle.html#border-prop
# Verified on Qt 6.11.2 (qml6, offscreen). qml6 crashes on a Behavior on a member of an
# attached value (`Drag.hotSpot.x`), so that has no scenario here. A Behavior on a member of a
# value (`font.pixelSize`) is in behavior-on-value-member.feature.

@qt-parity
Feature: A Behavior on an attached property or a member of a group
  `Behavior on` names a property of an attached object (`Layout.leftMargin`) or a member of a
  grouped property (`border.width`) as it names a property of the object
  itself. A value given to it is held back, and the animation takes the property there.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      Item {
        id: root; objectName: "root"
        property real margin: r.Layout.leftMargin
        property real border: r.border.width
        RowLayout {
          Rectangle {
            id: r
            Layout.leftMargin: 0
            Behavior on Layout.leftMargin { NumberAnimation { duration: 1000 } }
            border.width: 0
            Behavior on border.width { NumberAnimation { duration: 1000 } }
          }
        }
        function give() {
          r.Layout.leftMargin = 100
          r.border.width = 100
        }
      }
      """

  Scenario: The value given is held back
    When the method "give" of "root" is called
    Then the property "margin" of "root" is 0
    And the property "border" of "root" is 0

  Scenario: Half way the animation has taken the properties half way
    When the method "give" of "root" is called
    And 500 milliseconds pass
    Then the property "margin" of "root" is 50
    And the property "border" of "root" is 50

  Scenario: The animation ends at the value given
    When the method "give" of "root" is called
    And 1500 milliseconds pass
    Then the property "margin" of "root" is 100
    And the property "border" of "root" is 100
