# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-connections.html
#   https://doc.qt.io/qt-6/qml-qtqml-binding.html
#   https://doc.qt.io/qt-6/qml-qtquick-state.html
#   https://doc.qt.io/qt-6/qml-color.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (connections), buckets value:step:number-vs-list, value:step:list-vs-number
# and warning:assign(ljos-only):Unable_to_assign_null_to_color. A Binding without a value
# (value:step:undefined-ljos, value:step:string-vs-bool) is in features/states/binding.feature.

@qt-parity
Feature: What writes a property from outside its own binding
  A Connections runs its handlers when its target tells a change. A Binding holds a property
  while its `when` holds, and a state holds what its PropertyChanges give.

  # Backlog: when a handler runs beside the bindings that read the same property is the reactive
  # graph's scheduling.
  @backlog
  Scenario: A Connections' handler runs after the bindings that read the property
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int given
        property int read: given
        property string order: ""
        Connections {
          target: root
          function onReadChanged() { root.order += "read;" }
          function onGivenChanged() { root.order += "given;" }
        }
      }
      """
    When the property "given" of "root" is set to 3
    Then the property "order" of "root" is "read;given;"

  # qml6 gives a string, a date or a list<int> back when a Binding lets go, but leaves a var
  # holding a JS array or object as the Binding had it, even a var with a binding of its own;
  # the documented RestoreBindingOrValue gives each back, and Ljós does.
  @qt-diverges
  Scenario: A Binding of a list or an object, turned off later, gives back what it had
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var list: 0
        property var record: 0
        property var text: 0
        property string seen: [list.length, record.k, text].join(",")
        Binding { objectName: "b1"; target: root; property: "list"; value: ["a", "b"] }
        Binding { objectName: "b2"; target: root; property: "record"; value: ({k: 1}) }
        Binding { objectName: "b3"; target: root; property: "text"; value: "s" }
      }
      """
    When the property "when" of "b1" is set to false
    And the property "when" of "b2" is set to false
    And the property "when" of "b3" is set to false
    Then the property "seen" of "root" is ",,0"

  Scenario: A colour given null is an invalid colour, and nothing is warned
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property color c: null
        property bool valid: c.valid
      }
      """
    Then the property "valid" of "root" is false
    And the log has no warning matching "assign"
