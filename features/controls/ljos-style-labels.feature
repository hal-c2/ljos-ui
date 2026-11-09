# Sources:
#   docs/DIALECT.md (§4.16 the controls)
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html#icon-prop
#   https://doc.qt.io/qt-6/qtquickcontrols-icons.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-action.html
# Ljós's own style is Ljós's: Qt has no Ljos.Controls to compare it with, so
# every scenario is `@ljos-only`. What a Basic button shows of its icon and
# its mnemonic is held to qml6 in button-icon.feature and mnemonics.feature;
# the style shows the same of both.

@ljos-only
Feature: Ljós's own style: the icon and the text of a button and a menu row
  A button and a menu row of Ljos.Controls show the icon they are given as
  every style does: `icon.source`, or the icon of the action they follow,
  drawn in the colour of their text unless `icon.color` says otherwise.
  `iconName` gives one of the style's own icons instead, and wins where
  both are given. Their text is drawn as its mnemonic says: the letter
  after a lone "&" is underlined and the "&" is not drawn, and "&&" is one
  "&". The keys still read the text as written.

  Rule: A button shows the icon it is given

    Background:
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Templates as T
        import Ljos.Controls
        Rectangle {
          id: root; objectName: "root"
          width: 320; height: 120; color: "#ffffff"
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          Button { id: bare; objectName: "bare"; x: 10; y: 10; icon.source: "red.png" }
          Button { id: acting; objectName: "acting"; x: 60; y: 10; action: T.Action { icon.source: "red.png" } }
          Button { objectName: "tinted"; x: 110; y: 10; icon.source: "red.png"; icon.color: "#0066cc" }
          Button { objectName: "named"; x: 160; y: 10; iconName: "plus"; icon.source: "red.png" }
          Button { id: plain; x: 10; y: 60; text: "Save" }
          Button { id: pictured; x: 110; y: 60; text: "Save"; icon.source: "red.png" }
          readonly property real roomForIcon: pictured.implicitContentWidth - plain.implicitContentWidth
        }
        """

    @api-QtQuick.Controls.AbstractButton.icon.source
    Scenario: A button's icon is drawn at its start, in its text's colour
      Then the property "implicitContentWidth" of "bare" is 16
      And the pixel at 26, 24 of the window is "#000000"

    @api-QtQuick.Controls.AbstractButton.action
    Scenario: A button shows the icon of its action
      Then the property "implicitContentWidth" of "acting" is 16
      And the pixel at 76, 24 of the window is "#000000"

    @api-QtQuick.Controls.AbstractButton.icon.color
    Scenario: A button's icon is drawn in the colour it is given
      Then the pixel at 126, 24 of the window is "#0066cc"

    Scenario: A button with text keeps the icon's room and a space before it
      Then the property "roomForIcon" of "root" is 24

    Scenario: The style's own icon wins over the button's
      Then the property "implicitContentWidth" of "named" is 16
      And the pixel at 176, 24 of the window is not "#ff0000"

  Rule: A menu row shows the icon it is given, and the others keep its column

    Background:
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Templates as T
        import Ljos.Controls
        Item {
          width: 480; height: 240
          Menu {
            Component.onCompleted: open()
            MenuItem { objectName: "open"; text: "Open"; icon.source: "red.png" }
            MenuItem { objectName: "close"; text: "Close" }
          }
          Menu {
            x: 240
            Component.onCompleted: open()
            MenuItem { objectName: "save"; action: T.Action { text: "Save"; icon.source: "red.png" } }
            MenuItem { objectName: "quit"; text: "Quit" }
          }
          Menu {
            y: 120
            Component.onCompleted: open()
            MenuItem { objectName: "undo"; text: "Undo" }
          }
        }
        """

    @api-QtQuick.Controls.AbstractButton.icon.source
    Scenario: A row with an icon has it in the icons' column, and a row without keeps the column
      Then the property "iconSpace" of "close" is true
      And the property "contentItem.gutter" of "open" is 24
      And the property "contentItem.gutter" of "close" is 24
      And the property "iconSpace" of "undo" is false
      And the property "contentItem.gutter" of "undo" is 0

    @api-QtQuick.Controls.AbstractButton.action
    Scenario: A row's action gives it its icon
      Then the property "iconSpace" of "quit" is true
      And the property "contentItem.gutter" of "save" is 24
      And the property "contentItem.gutter" of "quit" is 24

  Rule: The text is drawn as its mnemonic says, and the keys read it as written

    Scenario: A button's "&" before a letter and its "&&" are not drawn
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 320; height: 200
          Button { id: a; text: "&Save" }
          Button { id: b; y: 40; text: "Save" }
          Button { id: c; y: 80; text: "Copy && Paste" }
          // A Text draws its "&" as written: one "&" is the room of "&&".
          Text { id: d; y: 120; font: c.font; text: "Copy & Paste" }
          readonly property bool markGone: a.implicitContentWidth > 0
                                           && a.implicitContentWidth === b.implicitContentWidth
          readonly property bool pairOne: c.implicitContentWidth > 0
                                          && c.implicitContentWidth === d.implicitWidth
        }
        """
      Then the property "markGone" of "root" is true
      And the property "pairOne" of "root" is true

    Scenario: A button's mnemonic letter is drawn underlined
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Rectangle {
          width: 120; height: 60; color: "#ffffff"
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          Button { padding: 0; background: null; font.pixelSize: 40; text: "&H" }
          Button { x: 60; padding: 0; background: null; font.pixelSize: 40; text: "H" }
        }
        """
      Then the pixel at 16, 39 of the window is within 120 of "#000000"
      And the pixel at 76, 39 of the window is within 10 of "#ffffff"

    Scenario: Alt and a Ljos button's letter clicks it
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Item {
          width: 300; height: 200
          Button { objectName: "b"; text: "&Save"; property int hits: 0; onClicked: hits++ }
        }
        """
      When the user presses "Alt+S"
      Then the property "hits" of "b" is 1

    Scenario: A menu row's "&" before a letter and its "&&" are not drawn
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 480; height: 400
          Menu {
            Component.onCompleted: open()
            MenuItem { id: marking; text: "&Open" }
            MenuItem { id: open; text: "Open" }
            MenuItem { id: pair; text: "Copy && Paste" }
          }
          Text { id: openText; font: open.font; text: "Open" }
          Text { id: pairText; font: open.font; text: "Copy & Paste" }
          readonly property bool markGone: marking.implicitContentWidth > 0
                                           && marking.implicitContentWidth === open.implicitContentWidth
          // A row grows by what its text grows by, gutter and keys alike.
          readonly property bool pairOne: Math.abs((pair.implicitContentWidth - open.implicitContentWidth)
                                                   - (pairText.implicitWidth - openText.implicitWidth)) < 0.01
        }
        """
      Then the property "markGone" of "root" is true
      And the property "pairOne" of "root" is true

    Scenario: A submenu's "&" before a letter is not drawn in its row
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 480; height: 400
          Menu { id: marked; Component.onCompleted: open(); Menu { title: "&Recent" } }
          Menu { id: unmarked; y: 200; Component.onCompleted: open(); Menu { title: "Recent" } }
          // `count` is read so that the row a menu makes for its submenu
          // is measured once it is there.
          function width(menu) {
            const item = menu.count > 0 ? menu.itemAt(0) : null
            return item ? item.implicitContentWidth : -1
          }
          readonly property bool markGone: width(marked) > 0 && width(marked) === width(unmarked)
        }
        """
      Then the property "markGone" of "root" is true
