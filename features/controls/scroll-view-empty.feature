# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollview.html#effectiveScrollBarWidth-prop
# The documentation says the effective width of the vertical bar is the width
# of the bar while it is visible. In Qt 6.11.2 a view nothing was declared in
# shows both bars and reports 0 for both until something makes a bar hide and
# show again (a policy set to AlwaysOff and back gives 10). Ljós says what the
# documentation says from the start: a layout that leaves room for the bars
# gets the same answer before and after.

@qt-diverges
Feature: The bars of a ScrollView with nothing in it
  A view reports the room its visible bars take, whether or not there is
  anything to scroll.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 250
        ScrollView { objectName: "view"; width: 200; height: 100 }
      }
      """

  @api-QtQuick.Controls.ScrollView.effectiveScrollBarWidth
  @api-QtQuick.Controls.ScrollView.effectiveScrollBarHeight
  Scenario: The bars that show take their room
    Then the property "ScrollBar.vertical.visible" of "view" is true
    And the property "ScrollBar.horizontal.visible" of "view" is true
    And the property "effectiveScrollBarWidth" of "view" is 10
    And the property "effectiveScrollBarHeight" of "view" is 10

  Scenario: A bar that was switched off and on again takes the room it took
    When the property "ScrollBar.vertical.policy" of "view" is set to 1
    Then the property "effectiveScrollBarWidth" of "view" is 0
    When the property "ScrollBar.vertical.policy" of "view" is set to 0
    Then the property "effectiveScrollBarWidth" of "view" is 10
