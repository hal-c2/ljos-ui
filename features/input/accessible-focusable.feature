# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html (focusable)
#   https://doc.qt.io/qt-6/qtquick-accessibility.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# The documentation's list of focusable roles has no StaticText; Qt 6.11.2
# answers true for it, and that is what is pinned here.

@qt-parity @a11y
Feature: Which items an assistive technology may give the focus
  Accessible.focusable starts out following the item's role and is the author's
  to set.

  Scenario Outline: The role decides whether an item starts out focusable
    Given the QML:
      """qml
      import QtQuick
      Rectangle { objectName: "it"; Accessible.role: Accessible.<role> }
      """
    Then the property "Accessible.focusable" of "it" is <focusable>

    Examples:
      | role         | focusable |
      | Button       | true      |
      | CheckBox     | true      |
      | RadioButton  | true      |
      | Switch       | true      |
      | MenuItem     | true      |
      | PageTab      | true      |
      | EditableText | true      |
      | SpinBox      | true      |
      | ComboBox     | true      |
      | Terminal     | true      |
      | ScrollBar    | true      |
      | StaticText   | true      |
      | Slider       | false     |
      | ToolBar      | false     |
      | Link         | false     |
      | ListItem     | false     |
      | ProgressBar  | false     |

  Rule: The default goes on with a role and never off again

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          function toSlider(name) { pick(name).Accessible.role = Accessible.Slider }
          function toButton(name) { pick(name).Accessible.role = Accessible.Button }
          function pick(name) {
            for (let i = 0; i < children.length; i++)
              if (children[i].objectName === name) return children[i]
          }
          Rectangle { objectName: "plain"; Accessible.name: "Plain" }
          Rectangle { objectName: "button"; Accessible.role: Accessible.Button }
          Rectangle {
            objectName: "off"
            Accessible.role: Accessible.Button
            Accessible.focusable: false
          }
        }
        """

    Scenario: An item with only a name is not focusable
      Then the property "Accessible.focusable" of "plain" is false

    Scenario: A role given later makes the item focusable
      When the method "toButton" of "root" is called with "plain"
      Then the property "Accessible.focusable" of "plain" is true

    Scenario: A button that becomes a slider stays focusable
      When the method "toSlider" of "root" is called with "button"
      Then the property "Accessible.focusable" of "button" is true

    Scenario: What the author assigned stays through a change of role
      When the method "toSlider" of "root" is called with "off"
      And the method "toButton" of "root" is called with "off"
      Then the property "Accessible.focusable" of "off" is false

    Scenario: Assigning it replaces the default
      When the property "Accessible.focusable" of "button" is set to false
      Then the property "Accessible.focusable" of "button" is false
