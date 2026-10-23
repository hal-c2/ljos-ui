# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qml-font.html
#   https://doc.qt.io/qt-6/qml-point.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A Behavior on a member of a value
  `Behavior on` names a member of a property that holds a value (`font.pixelSize`,
  `p.x` of a `point`) as it names a property. A value given to the member, or by a binding
  on it, is held back, and the animation takes the member there. The other members of the
  value are left as they are: one written while the animation runs is taken at once.
  (qml6 does otherwise for a point; see the last scenario.)

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real big: 10
        property real size: t.font.pixelSize
        property bool bold: t.font.bold
        property real bound: u.font.pixelSize
        property real px: r.p.x; property real py: r.p.y
        property alias running: anim.running
        Text {
          id: t; text: "x"; font.pixelSize: 10; font.bold: true
          Behavior on font.pixelSize { NumberAnimation { id: anim; duration: 1000 } }
        }
        Text {
          id: u; text: "y"; font.pixelSize: root.big
          Behavior on font.pixelSize { NumberAnimation { duration: 1000 } }
        }
        Item {
          id: r; property point p: Qt.point(0, 7)
          Behavior on p.x { NumberAnimation { duration: 1000 } }
        }
        function give() { t.font.pixelSize = 30 }
        function giveAll() { t.font.pixelSize = 30; root.big = 50; r.p.x = 100 }
        function unbold() { t.font.bold = false }
      }
      """

  Scenario: The value given to the member is held back
    When the method "give" of "root" is called
    Then the property "size" of "root" is 10
    And the property "running" of "root" is true

  Scenario: Half way the animation has taken the member half way
    When the method "give" of "root" is called
    And 500 milliseconds pass
    Then the property "size" of "root" is 20

  Scenario: The animation ends at the value given
    When the method "give" of "root" is called
    And 1500 milliseconds pass
    Then the property "size" of "root" is 30
    And the property "running" of "root" is false

  Scenario: A value a binding gives the member is animated too
    When the method "giveAll" of "root" is called
    Then the property "bound" of "root" is 10
    When 500 milliseconds pass
    Then the property "bound" of "root" is 30
    When 1000 milliseconds pass
    Then the property "bound" of "root" is 50

  Scenario: Another member written while the animation runs is taken at once
    When the method "give" of "root" is called
    And the method "unbold" of "root" is called
    Then the property "bold" of "root" is false
    And the property "size" of "root" is 10

  Scenario: Another member written while the animation runs stays when it ends
    When the method "give" of "root" is called
    And the method "unbold" of "root" is called
    And 1500 milliseconds pass
    Then the property "bold" of "root" is false
    And the property "size" of "root" is 30

  Scenario: A member of a point is animated
    When the method "giveAll" of "root" is called
    Then the property "px" of "root" is 0
    When 500 milliseconds pass
    Then the property "px" of "root" is within 5 of 50
    When 1000 milliseconds pass
    Then the property "px" of "root" is 100

  # qml6 sets the point's other member to 0 once the animation of `p.x` starts, where a
  # Behavior on a member of a font leaves the other members as they are. Ljós leaves them
  # on a point too.
  @qt-diverges
  Scenario: The other member of a point stays while one is animated
    When the method "giveAll" of "root" is called
    And 500 milliseconds pass
    Then the property "py" of "root" is 7
    When 1000 milliseconds pass
    Then the property "py" of "root" is 7
