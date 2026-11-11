# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#setSource-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#binding-method
#   https://doc.qt.io/qt-6/qtqml-documents-scope.html (an id is seen in the whole file)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A child object's bindings see the item's initial properties and later ids
  Loader.setSource(url, properties) sets the properties on the new item
  before its bindings run. A child object of that item whose bindings read
  such a property gets its value at once, as does one that reads an id
  declared further down the same file, and both are there when the item is
  complete. The child follows the property after.

  That holds whether the Loader makes the item at once or asynchronously,
  when setSource is called or once the Loader is made active after it, when
  setSource is called while the file that calls it is being made, and
  whether the child is a QML object or one of a type registered from Rust
  whose setter does more than store.

  Background:
    Given the file "Panel.qml":
      """qml
      import QtQuick
      Rectangle {
        id: panel
        objectName: "panel"
        property var source: null
        property string tab: ""
        property string log: ""
        QtObject {
          id: page
          property var held: panel.source
          property string tab: panel.tab
          property var view: later
          onHeldChanged: panel.log += "held" + (held ? held.objectName : held) + " "
        }
        property string seen: (page.held ? page.held.objectName : "null") + "/" + page.tab
                              + "/" + (page.view ? page.view.objectName : "null")
        Item { Item { Item { id: later; objectName: "later" } } }
        Component.onCompleted: log += "completed" + seen
      }
      """
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

  Scenario Outline: The child has the initial property and the later id when the item is complete, <how>
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
        // An inactive Loader keeps what setSource gives it and makes the
        // item once it is active.
        function load(later, active) {
          loader.asynchronous = later
          loader.active = active
          loader.setSource("Panel.qml", { source: Qt.binding(() => root.shown), tab: "t1" })
          loader.active = true
        }
        function showSecond() { shown = second }
      }
      """
    When the method "load" of "root" is called with <later>, <active>
    And 50 milliseconds pass
    Then the property "log" of "loader/panel" is "heldfirst completedfirst/t1/later"
    When the method "showSecond" of "root" is called
    Then the property "seen" of "loader/panel" is "second/t1/later"

    Examples:
      | how                               | later | active |
      | at once                           | false | true   |
      | asynchronously                    | true  | true   |
      | at once, made active after        | false | false  |
      | asynchronously, made active after | true  | false  |

  Scenario Outline: The child has them when the item is asked for as the file that asks is made, <how>
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
          Component.onCompleted: setSource(Qt.resolvedUrl("Panel.qml"), {
            source: Qt.binding(() => root.shown), tab: "t1"
          })
        }
        function showSecond() { shown = second }
      }
      """
    When 50 milliseconds pass
    Then the property "log" of "loader/panel" is "heldfirst completedfirst/t1/later"
    When the method "showSecond" of "root" is called
    Then the property "seen" of "loader/panel" is "second/t1/later"

    Examples:
      | how            | later |
      | at once        | false |
      | asynchronously | true  |

  @ljos-only
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

    Examples:
      | how                               | later | active |
      | at once                           | false | true   |
      | asynchronously                    | true  | true   |
      | at once, made active after        | false | false  |
      | asynchronously, made active after | true  | false  |

  @ljos-only
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

    Examples:
      | how            | later |
      | at once        | false |
      | asynchronously | true  |
