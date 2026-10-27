# Sources:
#   docs/ARCHITECTURE.md (section 5.4, the amendment for shipped files)
#   docs/research/13-aot-spike.md
# Compiling a file's bindings and handlers to Rust ahead of time is Ljós's
# own: Qt has nothing to compare it with. What the file does is the same
# either way, and that is what these scenarios hold it to.

@ljos-only
Feature: Files compiled ahead of time
  An application may have the bindings and handlers of the files it ships
  compiled to native code when it is built. Such a file behaves exactly as
  it would run dynamically: the same values, the same signals in the same
  order, the same at creation. A file that is not the one the code was made
  from, because it was edited while the application runs, runs dynamically,
  and so does any expression the compiler could not type from the file's
  declarations.

  Scenario Outline: A compiled file gives the values and signal order it gives dynamically (cross-check <check>)
    Given the files are compiled ahead of time
    And the compiled code cross-check is <check>
    And the QML:
      """qml
      import QtQuick
      Item {
          id: root
          objectName: "root"
          property int count: 0
          property real half: count / 2
          property string label: "n=" + count
          property string order: ""
          signal ping(int n)
          onPing: (n) => { order = order + "p" + n }
          onCountChanged: {
              order = order + "c"
              ping(count)
              order = order + "e"
          }
      }
      """
    When the property "count" of "root" is set to 3
    And the property "count" of "root" is set to 4
    Then the property "half" of "root" is 2
    And the property "label" of "root" is "n=4"
    And the property "order" of "root" is "cp3ecp4e"
    And the file "Scenario.qml" ran compiled code
    And the log has no warning matching "compiled code and the dynamic tiers differ"

    Examples:
      | check |
      | on    |
      | off   |

  Scenario: Changes made while a file is created are the same compiled
    Given the files are compiled ahead of time
    And the compiled code cross-check is off
    And the QML:
      """qml
      import QtQuick
      Item {
          id: root
          objectName: "root"
          property int count: 2
          property int doubled: count * 2
          property string order: ""
          onDoubledChanged: order = order + "d" + doubled
          Component.onCompleted: {
              order = order + "c"
              count = 5
          }
      }
      """
    Then the property "doubled" of "root" is 10
    And the property "order" of "root" is "d4cd10"
    And the file "Scenario.qml" ran compiled code

  Scenario: An expression the compiler cannot type runs dynamically
    Given the files are compiled ahead of time
    And the QML:
      """qml
      import QtQuick
      Item {
          id: root
          objectName: "root"
          property var items: [1, 2, 3]
          property int size: items.length
          property int twice: size * 2
          function grow() { items = [1, 2, 3, 4] }
      }
      """
    When the method "grow" of "root" is called
    Then the property "size" of "root" is 4
    And the property "twice" of "root" is 8
    And the file "Scenario.qml" ran compiled code

  Scenario: A signal argument is what the signal declares, compiled as dynamically
    Given the files are compiled ahead of time
    And the compiled code cross-check is on
    And the QML:
      """qml
      import QtQuick
      Item {
          id: root
          objectName: "root"
          property real r: 0
          property string order: ""
          signal ping(int n)
          onPing: (n) => { order = order + "p" + n }
          onRChanged: ping(r)
      }
      """
    When the property "r" of "root" is set to 2.5
    Then the property "order" of "root" is "p2"
    And the file "Scenario.qml" ran compiled code
    And the log has no warning matching "compiled code and the dynamic tiers differ"

  Scenario: A variable declared in a switch case is the cases' after it
    Given the files are compiled ahead of time
    And the compiled code cross-check is on
    And the QML:
      """qml
      import QtQuick
      Item {
          id: root
          objectName: "root"
          property int start: 0
          property string order: ""
          signal ping(int n)
          onPing: (n) => {
              switch (n) {
              case 1: let s = "a"
              case 2: s = s + "b"; order = s
              }
          }
          onStartChanged: ping(start)
      }
      """
    When the property "start" of "root" is set to 1
    Then the property "order" of "root" is "ab"
    And the file "Scenario.qml" ran compiled code
    And the log has no warning matching "compiled code and the dynamic tiers differ"

  Scenario: The check says which expressions run dynamically, and why
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
          property var items: [1, 2, 3]
          property int size: items.length
          property int twice: size * 2
      }
      """
    When the files are checked with statistics
    Then the check reports the lines:
      """
      compiled ahead of time  1 of 2 (50.0%)
      """
    And the check reports the lines:
      """
      Main.qml:4:30: runs dynamically: type: var property items
      """

  Scenario: A key of Qt's namespace is compiled ahead of time
    Given the files are compiled ahead of time
    And the compiled code cross-check is on
    And the QML:
      """qml
      import QtQuick
      Item {
          id: root
          objectName: "root"
          property int modifiers: 0
          property bool control: (modifiers & Qt.ControlModifier) !== 0
          property bool digit: modifiers === Qt.Key_1
      }
      """
    When the property "modifiers" of "root" is set to 67108864
    Then the property "control" of "root" is true
    And the property "digit" of "root" is false
    And the file "Scenario.qml" ran compiled code
    And the log has no warning matching "compiled code and the dynamic tiers differ"

  Scenario: The check compiles a key of Qt's namespace ahead of time
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
          property int modifiers: 0
          property bool control: (modifiers & Qt.ControlModifier) !== 0
      }
      """
    When the files are checked with statistics
    Then the check reports the lines:
      """
      compiled ahead of time  1 of 1 (100.0%)
      """

  Scenario: A singleton written in QML is read and written by compiled code
    Given the files are compiled ahead of time
    And the compiled code cross-check is on
    And the file "qmldir":
      """
      singleton Theme Theme.qml
      """
    And the file "Theme.qml":
      """qml
      pragma Singleton
      import QtQuick
      QtObject {
          property int base: 4
          property string name: "theme"
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
          id: root
          objectName: "root"
          property int step: 1
          property int scaled: Theme.base * step
          property string label: Theme.name + step
          onStepChanged: Theme.base += 1
      }
      """
    When the property "step" of "root" is set to 3
    Then the property "scaled" of "root" is 15
    And the property "label" of "root" is "theme3"
    And the file "Scenario.qml" ran compiled code
    And the log has no warning matching "compiled code and the dynamic tiers differ"

  Scenario: The check compiles what a file reads of a singleton the project declares
    Given the file "qmldir":
      """
      singleton Theme Theme.qml
      """
    And the file "Theme.qml":
      """qml
      pragma Singleton
      import QtQuick
      QtObject {
          property int base: 4
      }
      """
    And the file "Main.qml":
      """qml
      import QtQuick
      Item {
          property int step: 1
          property int scaled: Theme.base * step
      }
      """
    When the files are checked with statistics
    Then the check reports the lines:
      """
      compiled ahead of time  1 of 1 (100.0%)
      """

  Scenario: The check says a variable declared in a switch case runs dynamically
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
          property int n: 0
          property string order: ""
          function pick() {
              switch (n) {
              case 1: { let t = "a"; order = t; break }
              case 2: let s = "b"; order = s
              }
          }
      }
      """
    When the files are checked with statistics
    Then the check reports the lines:
      """
      Main.qml:5:11: runs dynamically: statement: a variable declared in a switch case
      """

  Rule: An edited file runs dynamically

    Background:
      Given the files are compiled ahead of time
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int count: 0
          property real half: count / 2
          property string order: ""
          onCountChanged: order = order + "a" + count
        }
        """

    Scenario: A file edited while the application runs gives what its new text says
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int count: 0
          property real half: count / 4
          property string order: ""
          onCountChanged: order = order + "b" + count
        }
        """
      And a reload happens
      And the property "count" of "root" is set to 2
      Then the property "half" of "root" is 0.5
      And the property "order" of "root" is "b2"
      And the file "Scenario.qml" ran no compiled code

    Scenario: A compiled handler replaced while the application runs keeps the values and the order
      When the property "count" of "root" is set to 1
      Then the file "Scenario.qml" ran compiled code
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int count: 0
          property real half: count / 2
          property string order: ""
          onCountChanged: order = order + "b" + count
        }
        """
      And a reload happens
      And the property "count" of "root" is set to 2
      Then the property "order" of "root" is "a1b2"
      And the property "half" of "root" is 1
      And the file "Scenario.qml" ran no compiled code
