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

  Rule: A menu keeps the icons' column only for the icons it shows

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.impl
        import Ljos.Controls
        Item {
          width: 480; height: 360
          Menu {
            Component.onCompleted: open()
            MenuItem { objectName: "hidden"; text: "Open"; iconName: "folder"; display: IconLabel.TextOnly }
            MenuItem { objectName: "other"; text: "Close" }
            MenuItem { objectName: "kept"; text: "Keep"; iconSpace: true }
          }
          Menu {
            x: 240
            Component.onCompleted: open()
            MenuItem { objectName: "lone"; iconName: "folder"; display: IconLabel.TextOnly }
            MenuItem { objectName: "beside"; text: "Close" }
          }
          Menu {
            y: 180
            Component.onCompleted: open()
            MenuItem { objectName: "marker"; text: "&"; iconName: "folder"; display: IconLabel.TextOnly }
            MenuItem { objectName: "besideMarker"; text: "Close" }
          }
        }
        """

    # A row shown as its text alone shows no icon, so the others keep no
    # column for it.
    @api-QtQuick.Controls.AbstractButton.display
    Scenario: A menu whose only icon is on a row shown as its text keeps no column
      Then the property "contentItem.gutter" of "hidden" is 0
      And the property "contentItem.gutter" of "other" is 0

    Scenario: A row told to keep the icons' column keeps it in a menu that keeps none
      Then the property "contentItem.gutter" of "kept" is 24

    # A row with no text shows its icon whatever its display.
    @api-QtQuick.Controls.AbstractButton.display
    Scenario: A row with no text shown as its text shows its icon, and the others keep its column
      Then the property "contentItem.gutter" of "lone" is 24
      And the property "contentItem.gutter" of "beside" is 24

    # A lone "&" is a mnemonic's marker and is not drawn, so the row has no
    # text and shows its icon.
    @api-QtQuick.Controls.AbstractButton.display
    Scenario: A row whose text is a lone "&" shows its icon, and the others keep its column
      Then the property "contentItem.gutter" of "marker" is 24
      And the property "contentItem.gutter" of "besideMarker" is 24

  Rule: A button shows what its display asks for

    Background:
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.impl
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 640; height: 60
          Button { id: plain; text: "Save" }
          Button { objectName: "beside"; x: 80; text: "Save"; iconName: "plus" }
          Button { objectName: "iconOnly"; x: 160; text: "Save"; iconName: "plus"; display: IconLabel.IconOnly }
          Button { objectName: "textOnly"; x: 240; text: "Save"; iconName: "plus"; display: IconLabel.TextOnly }
          Button { objectName: "pictureTextOnly"; x: 320; text: "Save"; icon.source: "red.png"; display: IconLabel.TextOnly }
          Button { objectName: "noIcon"; x: 400; text: "Save"; display: IconLabel.IconOnly }
          Button { objectName: "noText"; x: 480; iconName: "plus"; display: IconLabel.TextOnly }
          Button { objectName: "mnemonicOnly"; x: 560; text: "&"; icon.source: "red.png"; display: IconLabel.TextOnly }
          function part(c, name) { for (const k of c.contentItem.children) if (k.objectName === name) return k; return null }
          property var got: []
          // What it asks for besides its text, and whether it shows its icon and its text.
          function measure(name) {
            let c = null
            for (const k of root.children) if (k.objectName === name) c = k
            const label = part(c, "label").visible
            got = [c.implicitContentWidth - (label ? part(plain, "label").implicitWidth : 0),
                   [part(c, "glyph").visible || part(c, "picture").visible, label]]
          }
        }
        """

    # Each answer is set against what a button with the same text and no
    # icon asks for, so that no font's widths are written here. A button
    # with only one of the two shows it, whatever its display says; a lone
    # "&" is a mnemonic's marker and draws nothing, so it is no text.
    @api-QtQuick.Controls.AbstractButton.display
    Scenario Outline: A <control> shown <how> asks for the room of <what>
      When the method "measure" of "root" is called with "<control>"
      Then the property "got" of "root" is <got>

      Examples:
        | control         | how             | what                                   | got                 |
        | beside          | beside its text | its icon, the spacing and its text     | [24, [true, true]]  |
        | iconOnly        | as its icon     | its icon alone                         | [16, [true, false]] |
        | textOnly        | as its text     | its text alone                         | [0, [false, true]]  |
        | pictureTextOnly | as its text     | its text alone, though given a picture | [0, [false, true]]  |
        | noIcon          | as its icon     | its text, as it has no icon            | [0, [false, true]]  |
        | noText          | as its text     | its icon, as it has no text            | [16, [true, false]] |
        | mnemonicOnly    | as its text     | its picture, as "&" draws no text      | [16, [true, false]] |

  Rule: A button loads no picture it does not show

    Background:
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.impl
        import QtQuick.Templates as T
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 400; height: 60
          Button { objectName: "named"; text: "Save"; iconName: "plus"; icon.source: "red.png" }
          Button { objectName: "textOnly"; x: 80; text: "Save"; icon.source: "red.png"; display: IconLabel.TextOnly }
          Button { objectName: "acting"; x: 160; display: IconLabel.TextOnly; action: T.Action { text: "Save"; icon.source: "red.png" } }
          Button { objectName: "alone"; x: 240; icon.source: "red.png"; display: IconLabel.TextOnly }
          Button { objectName: "marker"; x: 320; text: "&"; icon.source: "red.png"; display: IconLabel.TextOnly }
          function part(c, name) { for (const k of c.contentItem.children) if (k.objectName === name) return k; return null }
          property var got: []
          // How large each one's picture was made: one never loaded is nothing.
          function measure() { got = Array.from(root.children).map((c) => part(c, "picture").implicitWidth) }
        }
        """

    # A glyph wins over the picture, and a display of the text alone hides
    # it where there is text; a button with no text that draws, none at all
    # or a lone "&", shows its picture.
    @api-QtQuick.Controls.AbstractButton.icon.source
    Scenario: A button whose glyph or display hides its picture loads none
      When the method "measure" of "root" is called
      Then the property "got" of "root" is [0, 0, 0, 16, 16]

    # The picture it shows then is the button's or its action's, from the
    # file that named it.
    @api-QtQuick.Controls.AbstractButton.action
    Scenario: A button shows the picture it hid once nothing hides it
      When the property "iconName" of "named" is set to ""
      And the property "display" of "textOnly" is set to 2
      And the property "display" of "acting" is set to 2
      And the method "measure" of "root" is called
      Then the property "got" of "root" is [16, 16, 16, 16, 16]

  Rule: A button shown under its icon has the two in its middle

    # The last answer is how far the middle of the block, from the top of
    # the icon to the foot of the text, is from the middle of the room
    # inside a button taller than it asks to be.
    @api-QtQuick.Controls.AbstractButton.display
    Scenario: A button shown under its icon asks for the wider of the two, the icon over the text in the middle, and half a spacing above and below
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.impl
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 240; height: 100
          Button { id: plain; text: "Save" }
          Button { id: under; x: 80; text: "Save"; iconName: "plus"; display: IconLabel.TextUnderIcon }
          Button { id: tall; x: 160; height: 100; text: "Save"; iconName: "plus"; display: IconLabel.TextUnderIcon }
          function part(c, name) { for (const k of c.contentItem.children) if (k.objectName === name) return k; return null }
          function middle(name) { const i = part(under, name); return Math.round(i.mapToItem(under, 0, 0).x + i.width / 2) }
          function block(c) { const g = part(c, "glyph"), l = part(c, "label"); return (g.mapToItem(c, 0, 0).y + l.mapToItem(c, 0, l.height).y) / 2 }
          property var got: []
          function measure() {
            const glyph = part(under, "glyph"), label = part(under, "label")
            got = [under.implicitContentWidth - part(plain, "label").implicitWidth,
                   under.implicitContentHeight - (16 + under.spacing + label.implicitHeight),
                   glyph.mapToItem(under, 0, 0).y + glyph.height <= label.mapToItem(under, 0, 0).y,
                   middle("glyph") - middle("label"), under.implicitHeight - under.implicitContentHeight,
                   Math.round(block(tall) - (tall.topPadding + tall.availableHeight / 2))]
          }
        }
        """
      When the method "measure" of "root" is called
      Then the property "got" of "root" is [0, 0, true, 0, 8, 0]

    # The padding is for the icon over the text: with only one of the two
    # shown, the button is one line, as beside its text.
    @api-QtQuick.Controls.Control.topPadding
    Scenario: A button shown under its icon with only its text or only its icon has no padding above or below
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.impl
        import Ljos.Controls
        Item {
          width: 200; height: 100
          Button { objectName: "underText"; text: "Save"; display: IconLabel.TextUnderIcon }
          Button { objectName: "underIcon"; x: 80; iconName: "plus"; display: IconLabel.TextUnderIcon }
        }
        """
      Then the property "topPadding" of "underText" is 0
      And the property "bottomPadding" of "underText" is 0
      And the property "topPadding" of "underIcon" is 0
      And the property "bottomPadding" of "underIcon" is 0

  Rule: A button given the padding of one edge keeps the other's

    # Beside its icon a button has no padding above or below; under it,
    # half a spacing each.
    @api-QtQuick.Controls.Control.bottomPadding
    Scenario: A button given its top padding keeps its bottom padding
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.impl
        import Ljos.Controls
        Item {
          width: 200; height: 100
          Button { objectName: "beside"; topPadding: 12; text: "Save" }
          Button { objectName: "under"; x: 80; topPadding: 12; text: "Save"; iconName: "plus"; display: IconLabel.TextUnderIcon }
        }
        """
      Then the property "bottomPadding" of "beside" is 0
      And the property "bottomPadding" of "under" is 4

  Rule: A button that shows its icon alone has it in its middle, as tall as it is drawn

    # Wider than it asks for, the icon is in the middle of the button, in
    # either direction, whether its display or an empty text leaves the
    # icon alone.
    @api-QtQuick.Controls.AbstractButton.display
    Scenario Outline: A button <width> wide with <what> <how> has it in its middle
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.impl
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 200; height: 60
          LayoutMirroring.enabled: <mirrored>
          LayoutMirroring.childrenInherit: true
          Button { id: button; x: 20; width: <width>; <icon>; <text>; display: IconLabel.<display> }
          property var got: []
          // How far the middle of the icon it shows is from the button's.
          function measure() {
            let mark = null
            for (const k of button.contentItem.children)
              if ((k.objectName === "glyph" || k.objectName === "picture") && k.visible) mark = k
            got = [Math.round(mark.mapToItem(button, 0, 0).x + mark.width / 2 - button.width / 2)]
          }
        }
        """
      When the method "measure" of "root" is called
      Then the property "got" of "root" is [0]

      Examples:
        | width | what                     | icon                   | text         | display        | how                         | mirrored |
        | 48    | its own icon             | iconName: "plus"       | text: "Save" | IconOnly       | shown as its icon           | false    |
        | 48    | a picture                | icon.source: "red.png" | text: "Save" | IconOnly       | shown as its icon           | false    |
        | 48    | its own icon             | iconName: "plus"       | text: "Save" | IconOnly       | shown as its icon, mirrored | true     |
        | 48    | its own icon and no text | iconName: "plus"       | text: ""     | TextBesideIcon | beside its text             | false    |

    # The icon's size is a limit to a picture, not its size: a smaller
    # picture is drawn as large as it is, and the button asks for that.
    @api-QtQuick.Controls.AbstractButton.icon.height
    Scenario: A button shown as its icon asks for the height its icon is drawn at
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.impl
        import Ljos.Controls
        Item {
          width: 200; height: 100
          Button { objectName: "small"; iconSize: 64; icon.source: "red.png"; text: "Save"; display: IconLabel.IconOnly }
          Button { objectName: "large"; x: 100; iconSize: 64; iconName: "plus"; text: "Save"; display: IconLabel.IconOnly }
        }
        """
      Then the property "implicitContentHeight" of "small" is 16
      And the property "implicitContentHeight" of "large" is 64

  Rule: A menu row shows what its display asks for

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.impl
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 480; height: 160
          MenuItem { objectName: "rowIconOnly"; width: 120; text: "Open"; iconName: "folder"; display: IconLabel.IconOnly }
          MenuItem { objectName: "rowTextOnly"; x: 120; width: 120; text: "Open"; iconName: "folder"; display: IconLabel.TextOnly }
          MenuItem { objectName: "rowNoIcon"; x: 240; width: 120; text: "Open"; display: IconLabel.IconOnly }
          MenuItem { objectName: "rowNoText"; x: 360; width: 120; iconName: "folder"; display: IconLabel.TextOnly }
          MenuItem { objectName: "rowMarker"; x: 120; y: 60; width: 120; text: "&"; iconName: "folder"; display: IconLabel.TextOnly }
          MenuItem { id: tall; y: 60; width: 120; text: "Open"; iconName: "folder"; display: IconLabel.IconOnly; font.pixelSize: 80 }
          readonly property bool tallFits: tall.implicitHeight === Theme.controlHeight
          function part(c, name) { for (const k of c.contentItem.children) if (k.objectName === name) return k; return null }
          property var got: []
          function measure(name) {
            let c = null
            for (const k of root.children) if (k.objectName === name) c = k
            got = [c.contentItem.gutter, [part(c, "glyph").visible || part(c, "picture").visible, part(c, "label").visible]]
          }
        }
        """

    # A row keeps its icons in the column the menu keeps for them, so the
    # text under the icon is drawn beside it; out of a menu, a row with no
    # icon shown keeps no column. A row with only one of the two shows it,
    # and a lone "&" draws no text.
    @api-QtQuick.Controls.AbstractButton.display
    Scenario Outline: A menu row shown <how> shows <what>
      When the method "measure" of "root" is called with "<control>"
      Then the property "got" of "root" is <got>

      Examples:
        | control     | how         | what                                      | got                 |
        | rowIconOnly | as its icon | its icon in the column                    | [24, [true, false]] |
        | rowTextOnly | as its text | its text, and no column                   | [0, [false, true]]  |
        | rowNoIcon   | as its icon | its text, as it has no icon               | [0, [false, true]]  |
        | rowNoText   | as its text | its icon in the column, as it has no text | [24, [true, false]] |
        | rowMarker   | as its text | its icon in the column, as "&" draws none | [24, [true, false]] |

    @api-QtQuick.Controls.AbstractButton.display
    Scenario: A menu row shown as its icon is as tall as a row, whatever the size of the text it hides
      Then the property "tallFits" of "root" is true

  Rule: A tab shows what its display asks for, in its middle

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.impl
        import Ljos.Controls
        Item {
          id: root; objectName: "root"; width: 640; height: 60
          TabButton { objectName: "tabIconOnly"; text: "Files"; iconName: "folder"; display: IconLabel.IconOnly }
          TabButton { objectName: "tabTextOnly"; x: 60; text: "Files"; iconName: "folder"; display: IconLabel.TextOnly }
          TabButton { objectName: "tabPlain"; x: 160; text: "Files" }
          TabButton { objectName: "tabNoIcon"; x: 260; text: "Files"; display: IconLabel.IconOnly }
          TabButton { objectName: "tabNoText"; x: 360; iconName: "folder"; display: IconLabel.TextOnly }
          TabButton { objectName: "tabUnder"; x: 420; text: "Files"; iconName: "folder"; display: IconLabel.TextUnderIcon }
          TabButton { objectName: "tabUnderText"; x: 480; text: "Files"; display: IconLabel.TextUnderIcon }
          TabButton { objectName: "tabUnderIcon"; x: 560; iconName: "folder"; display: IconLabel.TextUnderIcon }
          function part(i, name) { for (const k of i.children) { if (k.objectName === name) return k; const f = part(k, name); if (f) return f } return null }
          function middle(c, i) { return Math.round(i.mapToItem(c, 0, 0).x + i.width / 2) }
          property var got: []
          function measure(name) {
            let c = null; for (const k of root.children) if (k.objectName === name) c = k
            const glyph = part(c.contentItem, "glyph"), label = part(c.contentItem, "label"), lone = glyph.visible ? glyph : label
            got = name === "tabUnder"
                ? [glyph.mapToItem(c, 0, 0).y + glyph.height <= label.mapToItem(c, 0, 0).y, middle(c, glyph) - middle(c, label),
                   middle(c, label) - Math.round(c.width / 2), c.implicitHeight - c.implicitContentHeight]
                : [[glyph.visible, label.visible], c.implicitContentWidth - lone.width, middle(c, lone) - Math.round(c.width / 2)]
          }
        }
        """

    # What a tab shows alone is in its middle, and the tab asks for its
    # room alone. A tab with only one of the two shows it.
    @api-QtQuick.Controls.AbstractButton.display
    Scenario Outline: A tab shown <how> shows <what>
      When the method "measure" of "root" is called with "<control>"
      Then the property "got" of "root" is <got>

      Examples:
        | control     | how            | what                                                                            | got                   |
        | tabIconOnly | as its icon    | its icon alone, in its middle                                                   | [[true, false], 0, 0] |
        | tabTextOnly | as its text    | its text alone, in its middle                                                   | [[false, true], 0, 0] |
        | tabPlain    | with no icon   | its text alone, in its middle                                                   | [[false, true], 0, 0] |
        | tabNoIcon   | as its icon    | its text, as it has no icon                                                     | [[false, true], 0, 0] |
        | tabNoText   | as its text    | its icon, as it has no text                                                     | [[true, false], 0, 0] |
        | tabUnder    | under its icon | the icon over the text, in the tab's middle, and half a spacing above and below | [true, 0, 0, 8]       |

    # The padding is for the icon over the text: with only one of the two
    # shown, the tab is one line, as beside its text.
    @api-QtQuick.Controls.Control.verticalPadding
    Scenario: A tab shown under its icon with only its text or only its icon has no padding above or below
      Then the property "verticalPadding" of "tabUnderText" is 0
      And the property "verticalPadding" of "tabUnderIcon" is 0

  Rule: A tab shows the picture it or its action is given, as a button does

    # `iconName` wins over the picture, as on a button; a tab shown as its
    # text loads no picture.
    @api-QtQuick.Controls.AbstractButton.icon.source
    Scenario: A tab shown as its icon shows its picture or its action's, and one shown as its text loads none
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.impl
        import QtQuick.Templates as T
        import Ljos.Controls
        Item {
          id: root; objectName: "root"; width: 400; height: 60
          TabButton { objectName: "pictured"; text: "Files"; icon.source: "red.png"; display: IconLabel.IconOnly }
          TabButton { objectName: "acting"; x: 100; display: IconLabel.IconOnly; action: T.Action { text: "Files"; icon.source: "red.png" } }
          TabButton { objectName: "textOnly"; x: 200; text: "Files"; icon.source: "red.png"; display: IconLabel.TextOnly }
          function part(i, name) { for (const k of i.children) { if (k.objectName === name) return k; const f = part(k, name); if (f) return f } return null }
          property var got: []
          // Whether each shows its picture and its text, and how large its picture was made.
          function measure() {
            got = Array.from(root.children).map((c) => {
              const p = part(c.contentItem, "picture")
              return p === null ? null : [p.visible, part(c.contentItem, "label").visible, p.implicitWidth]
            })
          }
        }
        """
      When the method "measure" of "root" is called
      Then the property "got" of "root" is [[true, false, 12], [true, false, 12], [false, true, 0]]
      And the property "implicitContentWidth" of "pictured" is 12

    # An IconLabel draws its picture in `icon.color` where one is given and
    # in its `color` where none is, so the tab's ink tints the picture.
    @api-QtQuick.Controls.AbstractButton.icon.color
    Scenario: A tab's picture is drawn in the colour of its text
      Given a 16 by 16 image file "red.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.impl
        import Ljos.Controls
        Rectangle {
          width: 120; height: 60; color: "#ffffff"
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#0066cc" }
          TabButton { x: 10; y: 10; width: 40; height: 40; checked: true; icon.source: "red.png"; display: IconLabel.IconOnly }
          TabButton { x: 60; y: 10; width: 40; height: 40; icon.source: "red.png"; display: IconLabel.IconOnly }
        }
        """
      Then the pixel at 30, 30 of the window is "#0066cc"
      And the pixel at 80, 30 of the window is not "#ff0000"

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
