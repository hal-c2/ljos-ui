# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html#itemAt-method
#   https://doc.qt.io/qt-6/qml-qtqml-models-delegatechooser.html
# Measured on Qt 6.11.2 (qml6, offscreen), which does otherwise: see below.

# Qt 6.11 runs a binding that reads a Repeater's items through itemAt again only when count
# changes. When rows move, or a DelegateChooser makes a row's item again for a new role value,
# itemAt answers with the new items but the binding keeps what it saw before: "abc" after the
# move, "XY" after the role change, and "XY" after a role value no choice takes, where itemAt(0)
# is null; a binding that reads only whether there is an item still says "++" there. A binding
# that says something other than what it reads is stale, so in Ljós the binding follows the items.
@qt-diverges
Feature: A binding that reads a Repeater's items follows them when they change places
  A binding that walks a Repeater's items with itemAt runs again when its rows move, and when a
  row's item is made again or goes, even though count stays the same.

  Scenario: The rows move
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string order: {
          let s = ""
          for (let c = 0; c < rep.count; ++c) {
            const it = rep.itemAt(c)
            s += it ? it.objectName : "-"
          }
          return s
        }
        ListModel {
          id: rows
          objectName: "rows"
          ListElement { name: "a" }
          ListElement { name: "b" }
          ListElement { name: "c" }
        }
        Repeater {
          id: rep
          model: rows
          Item { required property string name; objectName: name; property string seen: root.order }
        }
      }
      """
    Then the property "order" of "root" is "abc"
    When the method "move" of "rows" is called with 0, 2, 1
    Then the property "order" of "root" is "bca"

  Scenario Outline: A DelegateChooser makes a row's item again
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      Item {
        id: root
        objectName: "root"
        function items() { return Array.from({ length: chosen.count }, (_, c) => chosen.itemAt(c)) }
        property string kinds: items().map(it => it ? it.objectName : "-").join("")
        // Reads nothing of the items, so only the Repeater can tell it that one went.
        property string present: items().map(it => it ? "+" : "-").join("")
        ListModel {
          id: typed
          objectName: "typed"
          ListElement { kind: "x" }
          ListElement { kind: "y" }
        }
        Repeater {
          id: chosen
          model: typed
          delegate: DelegateChooser {
            role: "kind"
            DelegateChoice { roleValue: "x"; Item { objectName: "X"; property string seen: root.kinds + root.present } }
            DelegateChoice { roleValue: "y"; Item { objectName: "Y"; property string seen: root.kinds + root.present } }
          }
        }
      }
      """
    Then the property "kinds" of "root" is "XY"
    And the property "present" of "root" is "++"
    When the method "setProperty" of "typed" is called with 0, "kind", "<kind>"
    Then the property "kinds" of "root" is "<kinds>"
    And the property "present" of "root" is "<present>"

    Examples:
      | kind | kinds | present |
      | y    | YY    | ++      |
      | z    | -Y    | -+      |
