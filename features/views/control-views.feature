# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html#delegateModel-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html#popup-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menu.html#contentModel-prop
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html#customizing-combobox
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: The views inside a combo box and a menu
  The rows of a combo box's popup and the items of a menu are shown by a ListView: the combo
  box's on its `delegateModel`, the menu's on its `contentModel`.

  Rule: The popup of a combo box is a ListView on its delegateModel

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string made: ""
          ComboBox {
            id: c; objectName: "c"
            x: 20; y: 20; width: 100; height: 30
            model: ["One", "Two", "Three"]
            property bool shown: popup.contentItem.model === delegateModel
            property bool delegated: delegateModel.delegate === delegate
            delegate: ItemDelegate {
              objectName: "d" + index; width: 100; height: 30; text: modelData
              property bool inside: parent === c.popup.contentItem.contentItem
              property bool viewed: ListView.view === c.popup.contentItem
              property bool current: ListView.isCurrentItem
              Component.onCompleted: root.made += index
            }
          }
        }
        """

    @qt-parity @api-QtQuick.Controls.ComboBox.delegateModel
    Scenario: The delegate model has the rows and the delegate of the combo box
      Then the property "delegateModel.count" of "c" is 3
      And the property "delegated" of "c" is true
      And the property "shown" of "c" is true

    @qt-parity @api-QtQuick.Controls.ComboBox.popup
    Scenario: The view of an open popup has the rows
      When the user touches 70, 35
      Then the property "popup.contentItem.count" of "c" is 3
      And the property "popup.contentItem.contentHeight" of "c" is 90
      And the property "popup.contentItem.height" of "c" is 90
      And the property "popup.contentItem.contentY" of "c" is 0
      And the property "popup.contentItem.clip" of "c" is true
      And the property "made" of "root" is "012"

    @qt-parity @api-QtQuick.Controls.ComboBox.delegate
    Scenario: A delegate is a row of the view
      When the user touches 70, 35
      Then the property "inside" of "d1" is true
      And the property "viewed" of "d1" is true
      And the property "z" of "d1" is 1
      And the property "y" of "d1" is 30

    @qt-parity @api-QtQuick.Controls.ComboBox.highlightedIndex
    Scenario: The view's current row is the highlighted one, with no animation between
      When the user touches 70, 35
      And the user presses "Down"
      Then the property "highlightedIndex" of "c" is 1
      And the property "popup.contentItem.currentIndex" of "c" is 1
      And the property "popup.contentItem.highlightMoveDuration" of "c" is 0
      And the property "current" of "d1" is true
      And the property "current" of "d0" is false

    @qt-parity
    Scenario: A closed popup's view has no current row
      When the user touches 70, 35
      And the user presses "Escape"
      Then the property "popup.visible" of "c" is false
      And the property "popup.contentItem.currentIndex" of "c" is -1

    # On Qt the view has the rows before the popup was ever opened: it counts them (`count` 3),
    # says how high they would be together (`contentHeight` 90) and, once its window is shown,
    # has made the first delegate to measure by. Ljós gives the view the delegate when the
    # popup first opens, so that a combo box nobody opens costs no delegate at all.
    @qt-diverges
    Scenario: A combo box that was never opened has made no delegate
      Then the property "made" of "root" is ""
      And the property "popup.contentItem.count" of "c" is 0
      And the property "popup.contentItem.contentHeight" of "c" is 0

  Rule: The view of a long popup makes the delegates near what it shows

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string made: ""
          ComboBox {
            id: c; objectName: "c"
            width: 100; height: 30
            model: 40
            delegate: ItemDelegate {
              width: 100; height: 30; text: modelData
              Component.onCompleted: root.made += "+" + index
            }
          }
        }
        """

    @qt-parity
    Scenario: Only the rows near the top are made when the popup opens at the first row
      When the user touches 50, 15
      And 1 second passes
      Then the property "popup.contentItem.count" of "c" is 40
      And the property "popup.contentItem.contentHeight" of "c" is 1200
      And the property "popup.contentItem.height" of "c" is 388
      And the property "made" of "root" is "+0+1+2+3+4+5+6+7+8+9+10+11+12+13+14+15+16+17+18+19+20+21+22+23"

  Rule: The content item of a menu is a ListView on its items

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          Menu {
            id: m; objectName: "m"
            MenuItem {
              objectName: "a"; text: "a"
              property bool inside: parent === m.contentItem.contentItem
            }
            MenuItem { objectName: "b"; text: "b" }
          }
          MenuItem { id: spare; objectName: "spare"; text: "c" }
          function add() { m.addItem(spare) }
        }
        """

    @qt-parity @api-QtQuick.Controls.Menu.contentModel
    Scenario: The view has one row for each item
      Then the property "contentItem.count" of "m" is 2
      And the property "contentItem.contentHeight" of "m" is 80
      And the property "contentItem.clip" of "m" is true
      And the property "inside" of "a" is true
      And the property "y" of "b" is 40
      And the property "z" of "b" is 1

    @qt-parity
    Scenario: A menu that fits in its window does not scroll
      When the method "open" of "m" is called
      Then the property "contentItem.interactive" of "m" is false
      And the property "contentItem.contentY" of "m" is 0

    @qt-parity @api-QtQuick.Controls.Menu.currentIndex
    Scenario: The view's current row is the menu's current item
      When the method "open" of "m" is called
      And the property "currentIndex" of "m" is set to 1
      Then the property "contentItem.currentIndex" of "m" is 1

    @qt-parity
    Scenario: An item added to an open menu is a row of the view
      When the method "open" of "m" is called
      And the method "add" of "root" is called
      And 100 milliseconds pass
      Then the property "contentItem.count" of "m" is 3
      And the property "contentItem.contentHeight" of "m" is 120
      And the property "y" of "spare" is 80
