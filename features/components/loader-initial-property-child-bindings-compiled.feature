# Sources:
#   docs/ARCHITECTURE.md (section 5.4, the amendment for shipped files)
#   features/components/loader-initial-property-child-bindings.feature
# Compiling ahead of time is Ljós's own: these are the native scenarios of
# loader-initial-property-child-bindings.feature, run from the code the
# files compiled to, as a shipped application runs them.

@ljos-only
Feature: A compiled child object's bindings see the item's initial properties and later ids
  A file shipped compiled ahead of time is made as it is when it runs
  dynamically: a child object whose bindings read a property that
  Loader.setSource gives the item, or an id declared further down the
  file, has both when the item is complete, and follows the property after.

  Background:
    Given the files are compiled ahead of time
    And the compiled code cross-check is on
    # Holder stands for a type an application registers in Rust: it has no
    # base type, held and view are var properties, and held's setter counts
    # its writes in writes a moment later, as a setter that hands its work
    # to the event loop does.
    And the file "NativePanel.qml":
      """qml
      import QtQuick
      import Fixtures
      Rectangle {
        id: panel
        objectName: "panel"
        property var source: null
        property string tab: ""
        property string log: ""
        Holder {
          id: page
          held: panel.source
          view: later
          onHeldChanged: panel.log += "held" + (held ? held.objectName : held) + " "
        }
        property bool written: page.writes > 0
        property string seen: (page.held ? page.held.objectName : "null") + "/" + panel.tab
                              + "/" + (page.view ? page.view.objectName : "null")
        Item { Item { Item { id: later; objectName: "later" } } }
        Component.onCompleted: log += "completed" + seen
      }
      """

  Scenario Outline: A native child's setter is given the initial property, <how>
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property QtObject shown: first
        property QtObject first: QtObject { objectName: "first" }
        property QtObject second: QtObject { objectName: "second" }
        Loader { id: loader; objectName: "loader" }
        function load(later, active) {
          loader.asynchronous = later
          loader.active = active
          loader.setSource("NativePanel.qml", { source: Qt.binding(() => root.shown), tab: "t1" })
          loader.active = true
        }
        function showSecond() { shown = second }
      }
      """
    When the method "load" of "root" is called with <later>, <active>
    And 50 milliseconds pass
    Then the property "log" of "loader/panel" is "heldfirst completedfirst/t1/later"
    And the property "written" of "loader/panel" is true
    When the method "showSecond" of "root" is called
    Then the property "seen" of "loader/panel" is "second/t1/later"
    And the file "NativePanel.qml" ran compiled code
    And the log has no warning matching "compiled code and the dynamic tiers differ"

    Examples:
      | how                               | later | active |
      | at once                           | false | true   |
      | asynchronously                    | true  | true   |
      | at once, made active after        | false | false  |
      | asynchronously, made active after | true  | false  |

  Scenario Outline: A native child's setter is given it when the item is asked for as the file that asks is made, <how>
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property QtObject shown: first
        property QtObject first: QtObject { objectName: "first" }
        property QtObject second: QtObject { objectName: "second" }
        Loader {
          objectName: "loader"
          asynchronous: <later>
          Component.onCompleted: setSource(Qt.resolvedUrl("NativePanel.qml"), {
            source: Qt.binding(() => root.shown), tab: "t1"
          })
        }
        function showSecond() { shown = second }
      }
      """
    When 50 milliseconds pass
    Then the property "log" of "loader/panel" is "heldfirst completedfirst/t1/later"
    And the property "written" of "loader/panel" is true
    When the method "showSecond" of "root" is called
    Then the property "seen" of "loader/panel" is "second/t1/later"
    And the file "NativePanel.qml" ran compiled code
    And the log has no warning matching "compiled code and the dynamic tiers differ"

    Examples:
      | how            | later |
      | at once        | false |
      | asynchronously | true  |
