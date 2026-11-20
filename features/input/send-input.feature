# Sources:
#   docs/ARCHITECTURE.md (§11: input, focus and IME)
#   https://github.com/hal-c2/ljos-ui/issues/622
# A native type hands one item an event of its own making
# (`ljos::input::send_input`): the key row of an on-screen keyboard types
# into a terminal that does not have the focus. The item gets the event as
# if it had the focus, which it is not given.
@ljos-only
Feature: A native type hands an item a key of its own
  The application sends a key to one item; that item hears it, and the
  item that has the focus neither hears it nor loses the focus.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        TextInput { id: field; objectName: "field"; width: 100; height: 20; focus: true }
        Item {
          objectName: "pad"
          property string got: ""
          Keys.onPressed: (event) => { got += event.text; event.accepted = true }
        }
      }
      """

  Scenario: A key sent to an item without the focus reaches it alone
    When the application sends "a" to "pad"
    Then the property "got" of "pad" is "a"
    And the property "text" of "field" is ""
    And the property "activeFocus" of "field" is true
