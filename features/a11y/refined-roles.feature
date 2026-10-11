# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   docs/research/05-accessibility-and-localisation.md (section 2.4)
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot read the accessibility tree through QML alone, so these
# scenarios run on Ljós only.

@ljos-only @a11y
Feature: Roles that a property makes more exact
  Qt has one role for every text field and one for every button. Where a
  screen reader knows the kinds apart, a property of Accessible says which.

  Scenario Outline: A property makes the role more exact
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        Rectangle {
          width: 80; height: 30
          Accessible.role: Accessible.<role>
          Accessible.name: "It"
          Accessible.<property>: true
        }
      }
      """
    Then the accessibility tree has a "<told as>" named "It"

    Examples:
      | role         | property      | told as            |
      | Button       | defaultButton | defaultbutton      |
      | EditableText | passwordEdit  | passwordinput      |
      | EditableText | searchEdit    | searchinput        |
      | EditableText | multiLine     | multilinetextinput |
      | ComboBox     | editable      | editablecombobox   |
      | List         | selectable    | listbox            |
      | ListItem     | selectable    | listboxoption      |

  Scenario Outline: A property that says nothing about a role leaves it
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        Rectangle {
          width: 80; height: 30
          Accessible.role: Accessible.<role>
          Accessible.name: "It"
          Accessible.<property>: true
        }
      }
      """
    Then the accessibility tree has a "<told as>" named "It"

    Examples:
      | role     | property      | told as  |
      | CheckBox | defaultButton | checkbox |
      | Button   | multiLine     | button   |
      | Slider   | editable      | slider   |

  Scenario: A field that hides what is typed is a password field before anything else
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        Rectangle {
          width: 80; height: 30
          Accessible.role: Accessible.EditableText
          Accessible.name: "It"
          Accessible.multiLine: true
          Accessible.searchEdit: true
          Accessible.passwordEdit: true
        }
      }
      """
    Then the accessibility tree has a "passwordinput" named "It"

  Scenario: The role follows the property
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        Rectangle {
          objectName: "it"
          width: 80; height: 30
          Accessible.role: Accessible.Button
          Accessible.name: "It"
        }
      }
      """
    And an assistive technology is listening
    When the property "Accessible.defaultButton" of "it" is set to true
    Then the accessibility tree has a "defaultbutton" named "It"
    And the accessibility tree has no "button" named "It"
