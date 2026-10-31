# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialogbuttonbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#contentWidth-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A button box as wide as the list of its buttons
  A style writes a box's content as a view over its buttons and makes the
  box as wide as the view. The box shares that width among the buttons,
  which sets the view's width again: a loop that Qt leaves at the width
  it reached first, each button its share of it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Templates as T
      Item {
        width: 600; height: 200
        component B: T.Button { implicitHeight: 20 }
        T.DialogButtonBox {
          id: box
          objectName: "box"
          spacing: 2; padding: 6
          implicitWidth: contentWidth + leftPadding + rightPadding
          implicitHeight: contentHeight + topPadding + bottomPadding
          contentWidth: (contentItem as ListView)?.contentWidth
          delegate: T.Button { implicitWidth: 20 + text.length * 8; implicitHeight: 24 }
          contentItem: ListView {
            implicitWidth: contentWidth
            model: box.contentModel; spacing: box.spacing
            orientation: ListView.Horizontal
          }
          B { objectName: "mine"; implicitWidth: 50; T.DialogButtonBox.buttonRole: T.DialogButtonBox.HelpRole }
          B { objectName: "wipe"; implicitWidth: 70; T.DialogButtonBox.buttonRole: T.DialogButtonBox.DestructiveRole }
          B { objectName: "aye"; implicitWidth: 40; T.DialogButtonBox.buttonRole: T.DialogButtonBox.YesRole }
          B { objectName: "none"; implicitWidth: 44 }
        }
      }
      """

  # In Qt a DialogButtonBox is a Container: its contentModel holds the
  # buttons and its content item places them. Ljós's has no contentModel
  # and places the buttons itself, so the ListView here has no rows and the
  # box is as wide as its padding.
  @backlog
  Scenario: The buttons share the width the list first asked for
    Then the property "contentWidth" of "box" is 406
    And the property "width" of "box" is 418
    And the property "availableWidth" of "box" is 406
    And the property "width" of "mine" is 100
    And the property "x" of "wipe" is 102
    And the property "width" of "none" is 100

  @backlog
  Scenario: Buttons the box makes share the same width with the others
    When the property "standardButtons" of "box" is set to 263168
    And 50 milliseconds pass
    Then the property "count" of "box" is 6
    And the property "contentWidth" of "box" is 406
    And the property "width" of "box" is 418
    And the property "width" of "mine" is 66
    And the property "width" of "wipe" is 66
    And the property "width" of "none" is 66
    And the property "x" of "none" is 340
