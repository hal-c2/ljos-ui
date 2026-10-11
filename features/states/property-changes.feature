# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-states.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: PropertyChanges
  A PropertyChanges says which properties a state gives new values or
  bindings, either of one `target` or of objects named by their ids.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property int n: 40
        Rectangle { id: box; objectName: "box"; width: 10; height: 10; border.width: 1 }
        Rectangle { id: other; objectName: "other"; x: 1 }
        Text { id: label; objectName: "label"; text: "abc"; width: 150 }
        states: [
          State { name: "target"
            PropertyChanges { target: box; width: 30; height: root.n; border.width: 4 } },
          State { name: "id"
            PropertyChanges { box.width: 44; box.height: root.n + 1; box.border.width: 3; other.x: 5 } },
          State { name: "once"
            PropertyChanges { target: box; explicit: true; width: root.n } },
          State { name: "kept"
            PropertyChanges { target: box; restoreEntryValues: false; width: 70 } },
          State { name: "reset"
            PropertyChanges { target: label; width: undefined } },
          State { name: "twice"
            PropertyChanges { target: box; width: 20 }
            PropertyChanges { target: box; width: 21 } },
          State { name: "wrong"
            PropertyChanges { target: box; nosuch: 1; width: 25 } }
        ]
      }
      """

  Rule: The changes are written for a target or for objects named by id

    @api-QtQuick.PropertyChanges.target
    Scenario Outline: With a target, <property> of the target is changed
      When the property "state" of "root" is set to "target"
      Then the property "<property>" of "box" is <value>

      Examples:
        | property     | value |
        | width        | 30    |
        | height       | 40    |
        | border.width | 4     |

    Scenario Outline: Named by id, <property> of "<object>" is changed
      When the property "state" of "root" is set to "id"
      Then the property "<property>" of "<object>" is <value>

      Examples:
        | object | property     | value |
        | box    | width        | 44    |
        | box    | height       | 41    |
        | box    | border.width | 3     |
        | other  | x            | 5     |

    Scenario Outline: Leaving the state puts <property> of "<object>" back
      When the property "state" of "root" is set to "id"
      And the property "state" of "root" is set to ""
      Then the property "<property>" of "<object>" is <value>

      Examples:
        | object | property     | value |
        | box    | width        | 10    |
        | box    | border.width | 1     |
        | other  | x            | 1     |

  Rule: A change may be a binding

    Scenario Outline: A binding of a state follows what it reads while the state lasts (<state>)
      When the property "state" of "root" is set to "<state>"
      And the property "n" of "root" is set to 60
      Then the property "height" of "box" is <height>

      Examples:
        | state  | height |
        | target | 60     |
        | id     | 61     |

    Scenario: A binding of a state no longer applies when the state is left
      When the property "state" of "root" is set to "target"
      And the property "state" of "root" is set to ""
      And the property "n" of "root" is set to 60
      Then the property "height" of "box" is 10

    @api-QtQuick.PropertyChanges.explicit
    Scenario: An explicit change takes the value its expression has when the state is entered
      When the property "state" of "root" is set to "once"
      And the property "n" of "root" is set to 60
      Then the property "width" of "box" is 40

    Scenario: A change to `undefined` resets the property
      When the property "state" of "root" is set to "reset"
      Then the property "width" of "label" is not 150
      And the property "width" of "label" is at most 100

    Scenario: A property that was reset is put back when the state is left
      When the property "state" of "root" is set to "reset"
      And the property "state" of "root" is set to ""
      Then the property "width" of "label" is 150

  Rule: What a state changed is put back unless it says otherwise

    @api-QtQuick.PropertyChanges.restoreEntryValues
    Scenario: A change that does not restore keeps its value when the state is left
      When the property "state" of "root" is set to "kept"
      And the property "state" of "root" is set to ""
      Then the property "width" of "box" is 70

  Rule: Changes are made in the order they are written

    Scenario: Of two changes of one property the last one counts
      When the property "state" of "root" is set to "twice"
      Then the property "width" of "box" is 21

    Scenario: Two changes of one property are both undone
      When the property "state" of "root" is set to "twice"
      And the property "state" of "root" is set to ""
      Then the property "width" of "box" is 10

  Rule: A change of a property the target does not have is reported

    Scenario: The property that is missing is named in a warning
      When the property "state" of "root" is set to "wrong"
      Then the log has a warning matching "PropertyChanges: Cannot assign to non-existent property .nosuch."

    Scenario: The other changes are made all the same
      When the property "state" of "root" is set to "wrong"
      Then the property "width" of "box" is 25
