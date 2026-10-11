# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html
#   https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html#enabled-attached-prop
#   https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html#childrenInherit-attached-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Taking an item's own mirroring back with undefined
  An item that was given LayoutMirroring.enabled keeps it whatever its
  ancestors say. Assigning undefined takes it back: the item is mirrored
  when an ancestor passes mirroring on, as if it had never been given
  one. childrenInherit has no such state and refuses undefined.

  Background:
    Given the file "Tree.qml":
      """qml
      import QtQuick
      Item {
        id: root
        width: 200; height: 100
        property bool above: false
        property bool own: false
        property bool passes: false
        property string seen
        property int changes: 0
        LayoutMirroring.enabled: above
        LayoutMirroring.childrenInherit: true
        Item {
          id: kid
          width: 50; height: 50
          anchors.left: parent.left
          LayoutMirroring.enabled: root.own
          LayoutMirroring.childrenInherit: root.passes
          LayoutMirroring.onEnabledChanged: root.changes++
          Rectangle { id: leaf; width: 10; height: 10; anchors.left: parent.left }
        }
        function look() { seen = kid.LayoutMirroring.enabled + "," + kid.x + "," + leaf.x }
        function back() { kid.LayoutMirroring.enabled = undefined; look() }
      }
      """

  Rule: The item follows its ancestors again

    Scenario Outline: An item that passes mirroring on is mirrored as its ancestors say at once
      Given the QML:
        """qml
        import QtQuick
        Tree { objectName: "tree"; above: <above>; own: <own>; passes: true }
        """
      When the method "look" of "tree" is called
      Then the property "seen" of "tree" is "<before>"
      When the method "back" of "tree" is called
      Then the property "seen" of "tree" is "<after>"

      Examples:
        | above | own   | before      | after       |
        | true  | true  | true,150,40 | true,150,40 |
        | true  | false | false,0,0   | true,150,40 |
        | false | true  | true,150,40 | false,0,0   |
        | false | false | false,0,0   | false,0,0   |

    Scenario Outline: From then on the item follows what its ancestors pass on
      Given the QML:
        """qml
        import QtQuick
        Tree { objectName: "tree"; above: <above>; own: <above> }
        """
      When the method "back" of "tree" is called
      Then the property "seen" of "tree" is "<after>"
      When the property "above" of "tree" is set to <turned>
      And the method "look" of "tree" is called
      Then the property "seen" of "tree" is "<then>"
      When the property "above" of "tree" is set to <above>
      And the method "look" of "tree" is called
      Then the property "seen" of "tree" is "<after>"

      Examples:
        | above | after       | turned | then        |
        | true  | true,150,40 | false  | false,0,0   |
        | false | false,0,0   | true   | true,150,40 |

    Scenario: An item that kept its own mirroring does not follow its ancestors
      Given the QML:
        """qml
        import QtQuick
        Tree { objectName: "tree"; above: true; own: true }
        """
      When the property "above" of "tree" is set to false
      And the method "look" of "tree" is called
      Then the property "seen" of "tree" is "true,150,0"

    Scenario: The change is signalled once, and taking back twice is the same as once
      Given the QML:
        """qml
        import QtQuick
        Tree {
          objectName: "tree"; above: true; own: false; passes: true
          property int first: -1
          property int second: -1
          function twice() {
            var n = changes
            back(); first = changes - n
            back(); second = changes - n - first
          }
        }
        """
      When the method "twice" of "tree" is called
      Then the property "first" of "tree" is 1
      And the property "second" of "tree" is 0
      And the property "seen" of "tree" is "true,150,40"

    Scenario: A value given after it was taken back is the item's own again
      Given the QML:
        """qml
        import QtQuick
        Tree {
          objectName: "tree"; above: true; own: true; passes: true
          function give() { back(); own = false; look() }
        }
        """
      When the method "give" of "tree" is called
      Then the property "seen" of "tree" is "true,150,40"

  Rule: childrenInherit is not taken back

    Scenario: Assigning undefined to childrenInherit is refused
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string said
          LayoutMirroring.childrenInherit: true
          function clear() {
            try { LayoutMirroring.childrenInherit = undefined } catch (e) { said = String(e) }
            said += "|" + LayoutMirroring.childrenInherit
          }
        }
        """
      When the method "clear" of "root" is called
      Then the property "said" of "root" is "Error: Cannot assign [undefined] to bool|true"
