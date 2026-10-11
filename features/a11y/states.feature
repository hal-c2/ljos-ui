# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qtquick-accessibility.html
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot read the accessibility tree through QML alone, so these
# scenarios run on Ljós only. expandable and expanded are documented for
# Qt 6.12; the oracle is Qt 6.11.2.

@ljos-only @a11y
Feature: What an item says about its state
  The properties of Accessible that say how an item is are told to an
  assistive technology with the item, and follow when they change.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 300; height: 200
        Rectangle {
          objectName: "it"
          width: 80; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: "It"
        }
      }
      """
    And an assistive technology is listening

  Scenario Outline: An item that says nothing has no state
    Then the "button" named "It" is not <state>

    Examples:
      | state             |
      | checkable         |
      | checked           |
      | partially checked |
      | pressed           |
      | expandable        |
      | expanded          |
      | selectable        |
      | selected          |
      | read-only         |
      | disabled          |

  Scenario Outline: A property that is set is a state of the item
    When the property "Accessible.<property>" of "it" is set to true
    Then the "button" named "It" is <state>

    Examples:
      | property        | state             |
      | checkable       | checkable         |
      | checked         | checked           |
      | checkStateMixed | partially checked |
      | pressed         | pressed           |
      | expandable      | expandable        |
      | expanded        | expanded          |
      | selectable      | selectable        |
      | selected        | selected          |
      | readOnly        | read-only         |

  Scenario Outline: A state goes when its property is cleared
    When the property "Accessible.<property>" of "it" is set to true
    And the property "Accessible.<property>" of "it" is set to false
    Then the "button" named "It" is not <state>

    Examples:
      | property        | state             |
      | checked         | checked           |
      | checkStateMixed | partially checked |
      | pressed         | pressed           |
      | expanded        | expanded          |
      | selected        | selected          |
      | readOnly        | read-only         |

  Scenario Outline: An item that has a state can have it
    When the property "Accessible.<property>" of "it" is set to true
    Then the "button" named "It" is <can>

    Examples:
      | property        | can        |
      | checked         | checkable  |
      | checkStateMixed | checkable  |
      | expanded        | expandable |
      | selected        | selectable |

  Scenario: An item that can be checked and is not says so
    When the property "Accessible.checkable" of "it" is set to true
    Then the "button" named "It" is not checked

  Scenario: A partially checked item is not said to be checked
    When the property "Accessible.checked" of "it" is set to true
    And the property "Accessible.checkStateMixed" of "it" is set to true
    Then the "button" named "It" is partially checked
    And the "button" named "It" is not checked

  Scenario: An item that is not enabled is disabled
    When the property "enabled" of "it" is set to false
    Then the "button" named "It" is disabled

  Scenario: An item inside one that is not enabled is disabled
    When the property "enabled" of "root" is set to false
    Then the "button" named "It" is disabled

  Scenario Outline: A role that is checkable by itself is told as checkable
    When the property "Accessible.role" of "it" is set to <number>
    Then the "<role>" named "It" is checkable
    And the "<role>" named "It" is not checked

    Examples:
      | number | role        |
      | 44     | checkbox    |
      | 45     | radiobutton |
      | 135    | switch      |

  Scenario: Static text is read-only
    When the property "Accessible.role" of "it" is set to 41
    Then the "label" named "It" is read-only
