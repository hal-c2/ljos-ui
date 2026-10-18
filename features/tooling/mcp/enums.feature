# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.9
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#horizontalAlignment-prop
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#acceptedButtons-prop
#   https://doc.qt.io/qt-6/qtqml-typesystem-enumerations.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Enums are read and written by name
  A property that holds an enum reads as a number in QML, and 4 says less
  than `Text.AlignHCenter` to whoever looks at it. The inspector shows such
  a value by the name it is written with in QML, the type that declares
  the enum before its key, and keeps the number alongside. Flags are the
  names of the keys they combine, joined by `|`. An agent sets such a
  property by the same name, with or without the type in front, or by
  the number.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        Text { objectName: "label"; text: "Hi"; horizontalAlignment: Text.AlignHCenter }
        MouseArea {
          objectName: "area"
          width: 50; height: 50
          acceptedButtons: Qt.LeftButton | Qt.RightButton
        }
      }
      """
    And the inspector is connected with the capabilities "inspect, mutate"

  Rule: An enum is shown by name, with its number

    Scenario: An enum reads as the type and key it is written with
      When the agent inspects "#label"
      Then the inspected property "horizontalAlignment" has the value "Text.AlignHCenter"
      And the inspected property "horizontalAlignment" has the number 4

    Scenario: Flags read as the keys they combine
      When the agent inspects "#area"
      Then the inspected property "acceptedButtons" has the value "Qt.LeftButton | Qt.RightButton"
      And the inspected property "acceptedButtons" has the number 3

  Rule: An enum is set by name or by number

    Scenario: An enum is set by its type and key
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#label", "property": "horizontalAlignment", "value": "Text.AlignRight"}
        """
      And the agent inspects "#label"
      Then the inspected property "horizontalAlignment" has the value "Text.AlignRight"

    Scenario: An enum is set by its key alone
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#label", "property": "horizontalAlignment", "value": "AlignLeft"}
        """
      And the agent inspects "#label"
      Then the inspected property "horizontalAlignment" has the number 1

    Scenario: An enum is set by its number written as text
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#label", "property": "horizontalAlignment", "value": "2"}
        """
      And the agent inspects "#label"
      Then the inspected property "horizontalAlignment" has the value "Text.AlignRight"

    Scenario: Flags are set by their keys joined by a bar
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#area", "property": "acceptedButtons", "value": "Qt.RightButton | Qt.MiddleButton"}
        """
      And the agent inspects "#area"
      Then the inspected property "acceptedButtons" has the value "Qt.RightButton | Qt.MiddleButton"

    Scenario: A name that is no key of the enum is refused
      When the agent calls the tool "app_set_property" with:
        """json
        {"target": "#label", "property": "horizontalAlignment", "value": "Text.AlignNowhere"}
        """
      Then the agent is refused with "invalid_argument"

  Rule: A trace tells an enum's name with its number

    Scenario: A traced change of an enum says its name
      When the agent calls the tool "app_trace" with:
        """json
        {"target": "#label"}
        """
      And the agent calls the tool "app_set_property" with:
        """json
        {"target": "#label", "property": "horizontalAlignment", "value": "Text.AlignRight"}
        """
      And the agent calls the tool "app_trace"
      Then the answer's "/events/0/name" is "horizontalAlignment"
      And the answer's "/events/0/value" is 2
      And the answer's "/events/0/key" is "Text.AlignRight"
