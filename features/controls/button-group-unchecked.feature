# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-buttongroup.html#checkedButton-prop

# Qt 6.11.2 goes on naming the button a script unchecked (checkedButton is
# still that button, though no button is checked). Its documentation says
# checkedButton is null when there is no checked button, which is what an
# application can rely on, so that is what happens in Ljós.
@qt-diverges
Feature: ButtonGroup whose checked button is unchecked
  A group with no checked button has no checkedButton.

  Scenario: With its checked button unchecked the group has no checked button
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        ButtonGroup { id: group; objectName: "group" }
        CheckBox { objectName: "a"; ButtonGroup.group: group; checked: true }
        CheckBox { objectName: "b"; y: 50; ButtonGroup.group: group }
      }
      """
    When the property "checked" of "a" is set to false
    Then the property "checkedButton" of "group" is null
    And the property "checked" of "b" is false
