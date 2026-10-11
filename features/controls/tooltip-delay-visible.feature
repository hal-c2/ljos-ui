# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tooltip.html
#
# On Qt 6.11.2 a tool tip with a delay that is told `visible: true` (written,
# bound or through the attached ToolTip.visible) reads `visible` false until
# the delay has passed. Ljós keeps the property at what it was given and holds
# back only the showing: `opened` and the signals wait for the delay as on Qt.
# The property is the only thing that says the tool tip is still wanted, and a
# binding that goes back to false during the delay has to be seen to call the
# tool tip off. open() and show() leave `visible` false during the delay on
# both (tooltip.feature).

@qt-diverges
Feature: ToolTip told to be visible during its delay
  A tool tip that was told to be visible keeps saying so while it waits for
  its delay; it shows when the delay has passed.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        width: 400; height: 400
        property string log: ""
        property bool wanted: false
        property string shown: "" + a.ToolTip.visible
        function show() { a.ToolTip.visible = true }
        Item {
          id: a
          x: 100; y: 150; width: 200; height: 100
          ToolTip.delay: 300
          ToolTip { objectName: "tip"; delay: 300
                    onOpened: root.log += "opened "; onClosed: root.log += "closed " }
          ToolTip { objectName: "bound"; delay: 300; visible: root.wanted
                    onOpened: root.log += "opened "; onClosed: root.log += "closed " }
        }
      }
      """

  @api-QtQuick.Controls.ToolTip.delay
  Scenario: A tool tip told to be visible says so while it waits
    When the property "visible" of "tip" is set to true
    Then the property "visible" of "tip" is true
    And the property "opened" of "tip" is false
    And the property "log" of "root" is ""
    When 500 milliseconds pass
    Then the property "opened" of "tip" is true
    And the property "log" of "root" is "opened "

  Scenario: A tool tip told not to be visible any more during its delay does not show
    When the property "visible" of "tip" is set to true
    And 100 milliseconds pass
    And the property "visible" of "tip" is set to false
    And 500 milliseconds pass
    Then the property "opened" of "tip" is false
    And the property "log" of "root" is ""

  Scenario: A binding that asks for the tool tip is waited for
    When the property "wanted" of "root" is set to true
    Then the property "visible" of "bound" is true
    And the property "opened" of "bound" is false
    When 500 milliseconds pass
    Then the property "opened" of "bound" is true
    And the property "log" of "root" is "opened "

  Scenario: A binding that stops asking during the delay calls the tool tip off
    When the property "wanted" of "root" is set to true
    And 100 milliseconds pass
    And the property "wanted" of "root" is set to false
    And 500 milliseconds pass
    Then the property "visible" of "bound" is false
    And the property "opened" of "bound" is false
    And the property "log" of "root" is ""

  @api-QtQuick.Controls.ToolTip.visible
  Scenario: The attached property says so while the shared tool tip waits
    When the method "show" of "root" is called
    Then the property "shown" of "root" is "true"
    When 500 milliseconds pass
    Then the property "shown" of "root" is "true"
