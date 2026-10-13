# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-slider.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dial.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-progressbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-checkdelegate.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-checkbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-rangeslider.html
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot see the accessibility tree through QML alone, so these
# scenarios run on Ljós only.

@ljos-only @a11y
Feature: Value controls to an assistive technology
  Each value control says what it is, what it is called and, where it
  has one, its value and the range the value is in; the delegates that
  check, choose or switch say they are check boxes, radio buttons and
  switches, as a screen reader works them.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 600; height: 600
        Slider { objectName: "slider"; Accessible.name: "Volume"; from: 0; to: 10; value: 4 }
        Dial { objectName: "dial"; y: 50; Accessible.name: "Gain"; value: 0.5 }
        ProgressBar { objectName: "bar"; y: 260; Accessible.name: "Copying"; value: 0.25 }
        BusyIndicator { objectName: "busy"; x: 300; Accessible.name: "Working" }
        ToolSeparator { objectName: "sep"; x: 400; Accessible.name: "Gap" }
        PageIndicator { objectName: "pages"; x: 450; count: 3; Accessible.name: "Pages" }
        DelayButton { objectName: "delay"; y: 300; text: "Launch" }
        RoundButton { objectName: "round"; x: 150; y: 300; text: "+" }
        CheckDelegate { objectName: "check"; y: 360; width: 200; text: "Wifi" }
        RadioDelegate { objectName: "radio"; y: 420; width: 200; text: "Small" }
        SwitchDelegate { objectName: "switch"; y: 480; width: 200; text: "Dark" }
        CheckBox { objectName: "box"; x: 300; y: 360; text: "All"; tristate: true }
        RangeSlider {
          objectName: "band"; x: 300; y: 480; Accessible.name: "Band"
          from: 0; to: 10; stepSize: 1; first.value: 2; second.value: 8
        }
      }
      """

  Scenario Outline: Each control says what it is
    Then the object "<object>" is exposed as a "<role>" named "<name>"

    Examples:
      | object | role              | name    |
      | slider | slider            | Volume  |
      | dial   | slider            | Gain    |
      | bar    | progressindicator | Copying |
      | busy   | meter             | Working |
      | sep    | splitter          | Gap     |
      | pages  | meter             | Pages   |
      | delay  | button            | Launch  |
      | round  | button            | +       |
      | check  | checkbox          | Wifi    |
      | radio  | radiobutton       | Small   |
      | switch | switch            | Dark    |

  Scenario: A slider tells its value and range
    Then the "slider" named "Volume" is at 4 in the range 0 to 10
    When the property "value" of "slider" is set to 7
    Then the "slider" named "Volume" is at 7 in the range 0 to 10

  Scenario: A dial and a progress bar tell theirs
    Then the "slider" named "Gain" is at 0.5 in the range 0 to 1
    And the "progressindicator" named "Copying" is at 0.25 in the range 0 to 1

  Scenario: A delegate that checks is checked as a check box is
    Then the "checkbox" named "Wifi" is not checked
    When the user clicks the "checkbox" "Wifi"
    Then the "checkbox" named "Wifi" is checked
    And the property "checked" of "check" is true

  Scenario: A switch delegate is checked as a switch is
    When the property "checked" of "switch" is set to true
    Then the "switch" named "Dark" is checked

  # A third state is "mixed" to a screen reader, as ARIA names it.
  @api-QtQuick.Controls.CheckDelegate.checkState
  Scenario: A check delegate that is partially checked says so
    When the property "tristate" of "check" is set to true
    And the property "checkState" of "check" is set to 1
    Then the "checkbox" named "Wifi" is partially checked
    And the "checkbox" named "Wifi" is not checked
    When the property "checkState" of "check" is set to 2
    Then the "checkbox" named "Wifi" is not partially checked
    And the "checkbox" named "Wifi" is checked

  @api-QtQuick.Controls.CheckBox.checkState
  Scenario: A check box that is partially checked says so
    When the property "checkState" of "box" is set to 1
    Then the "checkbox" named "All" is partially checked
    When the user clicks the "checkbox" "All"
    Then the "checkbox" named "All" is not partially checked
    And the "checkbox" named "All" is checked

  # Each handle of a range slider takes the keyboard of its own, so each
  # is a slider of its own inside the range slider, named after it.
  @api-QtQuick.Controls.RangeSlider.first @api-QtQuick.Controls.RangeSlider.second
  Scenario: Each handle of a range slider tells its own value
    Then the "slider" named "Band start" is at 2 in the range 0 to 10
    And the "slider" named "Band end" is at 8 in the range 0 to 10
    When the property "second.value" of "band" is set to 6
    Then the "slider" named "Band end" is at 6 in the range 0 to 10
    And the "slider" named "Band start" is at 2 in the range 0 to 10

  @api-QtQuick.Controls.RangeSlider.first @api-QtQuick.Controls.RangeSlider.second
  Scenario: An assistive technology moves one handle of a range slider
    When an assistive technology asks the "slider" named "Band start" to increase
    Then the property "first.value" of "band" is 3
    And the property "second.value" of "band" is 8
    When an assistive technology asks the "slider" named "Band end" to decrease
    Then the property "second.value" of "band" is 7
    And the property "first.value" of "band" is 3
    And the "slider" named "Band end" is at 7 in the range 0 to 10

  @api-QtQuick.Controls.RangeSlider.first
  Scenario: A handle of a range slider stops at the other handle
    When the property "first.value" of "band" is set to 7
    And an assistive technology asks the "slider" named "Band start" to increase
    And an assistive technology asks the "slider" named "Band start" to increase
    Then the property "first.value" of "band" is 8
    And the "slider" named "Band start" is at 8 in the range 0 to 10
