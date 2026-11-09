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
