# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-aliases
#   https://doc.qt.io/qt-6/qml-font.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An alias of a member of a value property
  An alias can name one member of a property whose value has members, `label.font.family` or
  `image.sourceSize.width`, as it names a property of a group, `box.border.width`. Reading the
  alias reads that member; giving the alias a value changes that member and keeps the others.

  Background:
    Given the file "Pill.qml":
      """qml
      import QtQuick
      Item {
        property alias mono: label.font.family
        property alias px: label.font.pixelSize
        property alias bw: box.border.width
        property alias wide: picture.sourceSize.width
        Text { id: label; objectName: "label"; text: "x"; font.pixelSize: 11; font.italic: true }
        Rectangle { id: box; objectName: "box" }
        Image { id: picture; objectName: "picture" }
        function grow() { label.font.pixelSize = 30 }
        function shrink() { px = 17 }
      }
      """

  Rule: The alias reads and writes the member it names

    Scenario: What the alias is given is the member's value
      Given the QML:
        """qml
        import QtQuick
        Item {
          Pill { objectName: "pill"; px: 20; mono: "monospace"; bw: 3; wide: 64 }
        }
        """
      Then the property "px" of "pill" is 20
      And the property "mono" of "pill" is "monospace"
      And the property "bw" of "pill" is 3
      And the property "wide" of "pill" is 64
      And the property "font.pixelSize" of "label" is 20
      And the property "font.family" of "label" is "monospace"
      And the property "border.width" of "box" is 3
      And the property "sourceSize.width" of "picture" is 64

    Scenario: The other members keep their values
      Given the QML:
        """qml
        import QtQuick
        Item {
          Pill { objectName: "pill"; mono: "monospace" }
        }
        """
      Then the property "font.pixelSize" of "label" is 11
      And the property "font.italic" of "label" is true

    Scenario: A change of the member is seen through the alias
      Given the QML:
        """qml
        import QtQuick
        Item {
          Pill {
            objectName: "pill"
            property int heard: 0
            onPxChanged: heard = px
          }
        }
        """
      When the method "grow" of "pill" is called
      Then the property "px" of "pill" is 30
      And the property "heard" of "pill" is 30

    Scenario: A script that writes the alias writes the member
      Given the QML:
        """qml
        import QtQuick
        Item {
          Pill { objectName: "pill" }
        }
        """
      When the method "shrink" of "pill" is called
      Then the property "font.pixelSize" of "label" is 17
      And the property "font.italic" of "label" is true

    Scenario: A binding on the alias follows what it reads
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property int size: 12
          Pill { objectName: "pill"; px: root.size * 2 }
        }
        """
      Then the property "font.pixelSize" of "label" is 24
      When the property "size" of "root" is set to 15
      Then the property "font.pixelSize" of "label" is 30

  Rule: Each member an alias binds keeps its own binding

    Scenario: Two aliases of one font are both bound
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property int size: 12
          property string face: "serif"
          Pill { objectName: "pill"; px: root.size * 2; mono: root.face }
        }
        """
      When the property "size" of "root" is set to 15
      And the property "face" of "root" is set to "sans"
      Then the property "font.pixelSize" of "label" is 30
      And the property "font.family" of "label" is "sans"

    Scenario: A write through one alias ends its binding, not the other's
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property int size: 12
          property string face: "serif"
          Pill { objectName: "pill"; px: root.size * 2; mono: root.face }
        }
        """
      When the method "shrink" of "pill" is called
      And the property "size" of "root" is set to 20
      And the property "face" of "root" is set to "sans"
      Then the property "font.pixelSize" of "label" is 17
      And the property "font.family" of "label" is "sans"
