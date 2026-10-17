# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#incubateObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What incubateObject says of the properties it is given
  An initial property that incubateObject cannot set is an error of the script
  that called it, told at the line of the call: Qt's words for the assignment
  that failed. The object is made all the same, with its own value there.
  createObject says it in words of its own (create-object-properties.feature).

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property var inc: null
        property int status: -1
        Component { id: kid; Item { objectName: "kid"; property int n: 1; readonly property int r: 2; property Item other; property url u } }
        Component { id: needy; Item { objectName: "needy"; required property int n } }
        function make(props) {
          inc = kid.incubateObject(root, props, Qt.Synchronous)
        }
        function later(props) { kid.incubateObject(root, props) }
        function create(props) { kid.createObject(root, props) }
        function need(now) { inc = needy.incubateObject(root, {}, now ? Qt.Synchronous : Qt.Asynchronous) }
        function look() { status = inc.status }
        function itself() { make({ n: root }) }
      }
      """

  Rule: A value that cannot be assigned is an error at the line of the call

    Scenario Outline: <props> is said to be <said>
      When the method "make" of "root" is called with <props>
      Then the property "n" of "root/kid" is 1
      And the log has a warning matching "^.*[.]qml:10: Error: Cannot assign <said>$"

      Examples:
        | props            | said                  |
        | {"n": "many"}    | QString to int        |
        | {"n": null}      | std::nullptr_t to int |
        | {"n": {}}        | QJSValue to int       |
        | {"other": 5}     | int to QQuickItem[*]  |
        | {"other": 1.5}   | double to QQuickItem[*] |
        | {"other": true}  | bool to QQuickItem[*] |
        | {"u": {}}        | QJSValue to QUrl      |

    Scenario: An object is a QObject to what it is assigned to
      When the method "itself" of "root" is called
      Then the log has a warning matching "^.*[.]qml:10: Error: Cannot assign QObject[*] to int$"

    Scenario: A property the object does not have is an error
      When the method "make" of "root" is called with {"nope": 1}
      Then the object "root/kid" exists
      And the log has a warning matching "^.*[.]qml:10: Error: Cannot assign to non-existent property .nope.$"

    Scenario: The line is that of the call when the object is made later
      When the method "later" of "root" is called with {"nope": 1}
      Then the object "root/kid" exists
      And the log has a warning matching "^.*[.]qml:12: Error: Cannot assign to non-existent property .nope.$"

  Rule: A read-only property keeps its value, where createObject sets it

    Scenario: incubateObject leaves a read-only property alone and says so
      When the method "make" of "root" is called with {"r": 7}
      Then the property "r" of "root/kid" is 2
      And the log has a warning matching "^.*[.]qml:10: TypeError: Cannot assign to read-only property .r.$"

    Scenario: createObject sets a read-only property
      When the method "create" of "root" is called with {"r": 7}
      Then the property "r" of "root/kid" is 7
      And the log has no warnings

  Rule: A path that leads nowhere is told by the component's file alone

    Scenario: incubateObject cannot resolve the path
      When the method "make" of "root" is called with {"nope.x": 1, "n": 4}
      Then the property "n" of "root/kid" is 4
      And the log has a warning matching "^.*[.]qml: Cannot resolve property .nope[.]x.[.]$"

    Scenario: createObject says nothing of it
      When the method "create" of "root" is called with {"nope.x": 1, "n": 4}
      Then the property "n" of "root/kid" is 4
      And the log has no warnings

  Rule: A required property left out ends the incubation in Error, and nothing is said

    Scenario Outline: An incubation <how> ends in Error
      When the method "need" of "root" is called with <now>
      And the method "look" of "root" is called
      Then the property "status" of "root" is 3
      And the object "root/needy" does not exist
      And the log has no warnings

      Examples:
        | how               | now   |
        | made at once      | true  |
        | made in slices    | false |
