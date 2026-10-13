# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tabbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tabbutton.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: TabBar
  A tab bar is a row of tab buttons, one of which is checked: the current
  one. The bar shares its width among the buttons that were given none,
  and every button is as high as the highest asks.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 200
        property string seen: ""
        TabBar {
          id: bar; objectName: "bar"; width: 301
          TabButton { id: one; objectName: "one"; text: "One" }
          TabButton { id: two; objectName: "two"; text: "Two" }
          TabButton { id: three; objectName: "three"; text: "Three" }
        }
        TabBar {
          id: fixed; objectName: "fixed"; y: 50
          TabButton { objectName: "wide"; text: "A"; width: 100 }
          TabButton { objectName: "auto"; text: "B" }
        }
        TabBar { id: none; objectName: "none"; y: 100 }
        function attached() { seen = one.TabBar.index + " " + three.TabBar.index + " " + (two.TabBar.tabBar === bar) + " " + one.TabBar.position }
        function takeTwo() {
          var t = bar.takeItem(1)
          seen = t.TabBar.index + " " + t.TabBar.tabBar
        }
      }
      """

  @api-QtQuick.Controls.TabBar.currentIndex
  Scenario: The first button is the current one
    Then the property "currentIndex" of "bar" is 0
    And the property "checked" of "one" is true
    And the property "checked" of "two" is false

  Scenario: The buttons share the width of the bar
    Then the property "width" of "one" is within 0.01 of 99.67
    And the property "x" of "two" is within 0.01 of 100.67
    And the property "height" of "one" is 40
    And the property "height" of "bar" is 40

  @api-QtQuick.Controls.TabBar.contentWidth
  Scenario: The bar asks for what its buttons ask
    Then the property "contentHeight" of "bar" is 40
    And the property "contentWidth" of "fixed" is at least 113

  Scenario: A button that was given a width keeps it
    Then the property "width" of "wide" is 100

  Scenario: An empty bar is empty
    Then the property "contentWidth" of "none" is 0
    And the property "contentHeight" of "none" is 0
    And the property "currentIndex" of "none" is -1

  Scenario: Checking a button makes it the current one
    When the property "checked" of "three" is set to true
    Then the property "currentIndex" of "bar" is 2
    And the property "checked" of "one" is false

  Scenario: Setting the current index checks its button
    When the property "currentIndex" of "bar" is set to 1
    Then the property "checked" of "two" is true
    And the property "checked" of "one" is false

  Scenario: Clicking a tab makes it current
    When the user presses the pointer at 150, 20
    And the user releases the pointer at 150, 20
    Then the property "currentIndex" of "bar" is 1

  @api-QtQuick.Controls.TabBar.index @api-QtQuick.Controls.TabBar.tabBar @api-QtQuick.Controls.TabBar.position
  Scenario: A button knows its place in the bar
    When the method "attached" of "root" is called
    Then the property "seen" of "root" is "0 2 true 0"

  Scenario: A button taken out of the bar is in no bar
    When the method "takeTwo" of "root" is called
    Then the property "seen" of "root" is "-1 null"

  @api-QtQuick.Controls.TabButton.checkable
  Scenario: A tab button is checkable and exclusive
    Then the property "checkable" of "one" is true
    And the property "autoExclusive" of "one" is true
    And the property "focusPolicy" of "one" is 11
    And the property "focusPolicy" of "bar" is 0
