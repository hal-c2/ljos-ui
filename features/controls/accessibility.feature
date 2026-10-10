# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qtquickcontrols-accessibility.html
# The Qt oracle cannot read default accessible names through QML alone, so
# these scenarios run on Ljós only.

@ljos-only @a11y
Feature: What the controls tell an assistive technology
  A control has a role by its type and a name from its text or title, without
  the application saying anything. What the application does say wins.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        objectName: "win"
        width: 400; height: 300
        property int saved: 0
        header: ToolBar { objectName: "bar"; ToolButton { text: "Back" } }
        Column {
          Button { objectName: "save"; text: "Save"; onClicked: saved++ }
          CheckBox { objectName: "remember"; text: "Remember me" }
          RadioButton { text: "Daily" }
          Switch { text: "Wi-Fi" }
          Label { text: "Volume" }
          GroupBox { title: "Network"; Label { text: "Offline" } }
          Button {
            objectName: "own"; text: "Shown"
            Accessible.name: "Spoken"
          }
        }
      }
      """

  Scenario Outline: A control has the role of its type and is named by its text
    Then the accessibility tree has a "<role>" named "<name>"

    Examples:
      | role        | name        |
      | button      | Save        |
      | button      | Back        |
      | checkbox    | Remember me |
      | radiobutton | Daily       |
      | switch      | Wi-Fi       |
      | label       | Volume      |
      | group       | Network     |
      | toolbar     |             |

  Scenario: The node of a control is the control
    Then the object "save" is exposed as a "button" named "Save"

  Scenario: The name follows the text
    When the property "text" of "save" is set to "Store"
    Then the accessibility tree has a "button" named "Store"
    And the accessibility tree has no "button" named "Save"

  Scenario: A name the application gives wins over the text
    Then the accessibility tree has a "button" named "Spoken"
    And the accessibility tree has no "button" named "Shown"

  Scenario: A role the application gives wins over the role of the type
    When the property "Accessible.role" of "remember" is set to 43
    Then the accessibility tree has a "button" named "Remember me"

  Scenario: What is in a group box is inside its group
    Then the "label" named "Offline" is inside the "group" named "Network"

  Scenario: A button in a tool bar is inside the tool bar
    Then the "button" named "Back" is inside the "toolbar" named ""

  Scenario: A control found by role and name can be clicked
    When the user clicks the "button" "Save"
    Then the property "saved" of "win" is 1

  Scenario: A check box found by role and name can be checked
    When the user clicks the "checkbox" "Remember me"
    Then the property "checked" of "remember" is true
