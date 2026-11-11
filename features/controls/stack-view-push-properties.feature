# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-stackview.html#push-method
# Verified on Qt 6.11.2 and 6.12.0 (qml6, offscreen).

@qt-parity
Feature: The properties a StackView push gives its page
  The properties given with a page are set on it as incubateObject sets
  them, whether the page is a Component, a file or an item: one the page
  does not have is an error of the script that pushed, told at the line of
  the push, and the page is pushed all the same with the others set. A
  value of the wrong type is told the same way, and a path that leads
  nowhere by the pushing file's name alone.

  Background:
    Given the file "Page.qml":
      """qml
      import QtQuick
      Item { objectName: "page"; property int k: 0 }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string seen: ""
        Component { id: comp; Item { objectName: "page"; property int k: 0 } }
        Item { id: given; objectName: "page"; property int k: 0 }
        StackView { id: stack; objectName: "stack"; anchors.fill: parent }
        function look() { seen = stack.depth + ":" + stack.currentItem.k }
        function pushComponent(props) { stack.push(comp, props, StackView.Immediate); look() }
        function pushFile(props) { stack.push("Page.qml", props, StackView.Immediate); look() }
        function pushItem(props) { stack.push(given, props, StackView.Immediate); look() }
      }
      """

  Scenario Outline: A property the page does not have is told at the push, <page>
    When the method "<method>" of "root" is called with {"nope": 1, "k": 2}
    Then the property "seen" of "root" is "1:2"
    And the log has a warning matching "^.*[.]qml:<line>: Error: Cannot assign to non-existent property .nope.$"

    Examples:
      | page        | method        | line |
      | a Component | pushComponent | 10   |
      | a file      | pushFile      | 11   |
      | an item     | pushItem      | 12   |

  Scenario Outline: A value of the wrong type is told at the push, <page>
    When the method "<method>" of "root" is called with {"k": "many"}
    Then the property "seen" of "root" is "1:0"
    And the log has a warning matching "^.*[.]qml:<line>: Error: Cannot assign QString to int$"

    Examples:
      | page        | method        | line |
      | a Component | pushComponent | 10   |
      | a file      | pushFile      | 11   |

  Scenario: A path that leads nowhere is told by the pushing file's name
    When the method "pushComponent" of "root" is called with {"nope.x": 1, "k": 5}
    Then the property "seen" of "root" is "1:5"
    And the log has a warning matching "^(.*/)?[^/:]*[.]qml: Cannot resolve property .nope[.]x.[.]$"
