# Sources:
#   docs/DIALECT.md §4.13 (Ljós's names for a Text's members) and §4.14
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.pixelSize-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.bold-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#horizontalAlignment-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#wrapMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#textFormat-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#elide-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Each `@qt-parity` file is Qt's QML and is the oracle; the `@ljos-only` file beside it says the
# same in Ljós's API and is held to its numbers. The enum values keep Qt's names for now
# (`Text.WordWrap`, `Font.Bold`): what Ljós renames here is the members.

Feature: A Text in Ljós's API
  A font's size is `size`, in logical pixels, and its boldness is its
  `weight`. A Text's alignment is `align`, its wrapping `wrap` and its
  format `format`; `elide` keeps its name. A change signal and its handler
  follow the member they are about. `ljos migrate` writes a Qt file's
  Text in these names, and the file does what it did.

  Rule: A font's size is `size`

    @qt-parity
    Scenario: A font's pixel size, read and written
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int size: big.font.pixelSize
          property bool bigIsTaller: big.height > plain.height
          property bool grownIsBig: grown.height === big.height
          Text { id: plain; text: "Size"; font.pixelSize: 20 }
          Text { id: big; text: "Size"; font.pixelSize: 40 }
          Text { id: grown; text: "Size"; font.pixelSize: 20 }
          Component.onCompleted: grown.font.pixelSize = 40
        }
        """
      When 100 milliseconds pass
      Then the property "size" of "root" is 40
      And the property "bigIsTaller" of "root" is true
      And the property "grownIsBig" of "root" is true

    @ljos-only
    Scenario: The same size, as `size`
      Given the QML:
        """qml
        import Ljos.Ui; import Ljos.Core
        Item {
          objectName: "root"
          property int size: big.font.size
          property bool bigIsTaller: big.height > plain.height
          property bool grownIsBig: grown.height === big.height
          Text { id: plain; text: "Size"; font.size: 20 }
          Text { id: big; text: "Size"; font.size: 40 }
          Text { id: grown; text: "Size"; font.size: 20 }
          Component.onCompleted: grown.font.size = 40
        }
        """
      When 100 milliseconds pass
      Then the property "size" of "root" is 40
      And the property "bigIsTaller" of "root" is true
      And the property "grownIsBig" of "root" is true

  Rule: A font's boldness is its `weight`

    @qt-parity
    Scenario: A bold font is wider, and weighs 700
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int weight: bold.font.weight
          property bool boldIsWider: bold.width > plain.width
          Text { id: plain; text: "Weight"; font.pixelSize: 20 }
          Text { id: bold; text: "Weight"; font.pixelSize: 20; font.bold: true }
        }
        """
      When 100 milliseconds pass
      Then the property "weight" of "root" is 700
      And the property "boldIsWider" of "root" is true

    @ljos-only
    Scenario: The same font, given its weight
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          objectName: "root"
          property int weight: bold.font.weight
          property bool boldIsWider: bold.width > plain.width
          Text { id: plain; text: "Weight"; font.size: 20 }
          Text { id: bold; text: "Weight"; font.size: 20; font.weight: Font.Bold }
        }
        """
      When 100 milliseconds pass
      Then the property "weight" of "root" is 700
      And the property "boldIsWider" of "root" is true

  Rule: A Text's alignment is `align`

    @qt-parity
    Scenario: horizontalAlignment, read, written and heard
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property int align: label.horizontalAlignment
          property int heard: 0
          Text {
            id: label
            objectName: "label"
            width: 200
            text: "Align"
            horizontalAlignment: Text.AlignHCenter
            onHorizontalAlignmentChanged: root.heard++
          }
          Component.onCompleted: label.horizontalAlignment = Text.AlignRight
        }
        """
      When 100 milliseconds pass
      Then the property "align" of "root" is 2
      And the property "heard" of "root" is 1

    @ljos-only
    Scenario: The same alignment, as `align`
      Given the QML:
        """qml
        import Ljos.Ui; import Ljos.Core
        Item {
          id: root
          objectName: "root"
          property int align: label.align
          property int heard: 0
          Text {
            id: label
            objectName: "label"
            width: 200
            text: "Align"
            align: Text.AlignHCenter
            onAlignChanged: root.heard++
          }
          Component.onCompleted: label.align = Text.AlignRight
        }
        """
      When 100 milliseconds pass
      Then the property "align" of "root" is 2
      And the property "heard" of "root" is 1

  Rule: A Text's wrapping is `wrap`

    @qt-parity
    Scenario: wrapMode breaks a long line
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int mode: wrapped.wrapMode
          property bool wraps: wrapped.lineCount > 1
          property bool oneLine: plain.lineCount === 1
          Text { id: wrapped; width: 60; text: "one two three four"; wrapMode: Text.WordWrap }
          Text { id: plain; width: 60; text: "one two three four" }
        }
        """
      When 100 milliseconds pass
      Then the property "mode" of "root" is 1
      And the property "wraps" of "root" is true
      And the property "oneLine" of "root" is true

    @ljos-only
    Scenario: The same line, broken by `wrap`
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          objectName: "root"
          property int mode: wrapped.wrap
          property bool wraps: wrapped.lineCount > 1
          property bool oneLine: plain.lineCount === 1
          Text { id: wrapped; width: 60; text: "one two three four"; wrap: Text.WordWrap }
          Text { id: plain; width: 60; text: "one two three four" }
        }
        """
      When 100 milliseconds pass
      Then the property "mode" of "root" is 1
      And the property "wraps" of "root" is true
      And the property "oneLine" of "root" is true

  Rule: A Text's format is `format`

    @qt-parity
    Scenario: textFormat reads the tags as markup
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int format: rich.textFormat
          property bool richIsNarrower: rich.width < plain.width
          Text { id: rich; text: "<b>Tags</b>"; textFormat: Text.RichText }
          Text { id: plain; text: "<b>Tags</b>"; textFormat: Text.PlainText }
        }
        """
      When 100 milliseconds pass
      Then the property "format" of "root" is 1
      And the property "richIsNarrower" of "root" is true

    @ljos-only
    Scenario: The same markup, read by `format`
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          objectName: "root"
          property int format: rich.format
          property bool richIsNarrower: rich.width < plain.width
          Text { id: rich; text: "<b>Tags</b>"; format: Text.RichText }
          Text { id: plain; text: "<b>Tags</b>"; format: Text.PlainText }
        }
        """
      When 100 milliseconds pass
      Then the property "format" of "root" is 1
      And the property "richIsNarrower" of "root" is true

  Rule: `elide` keeps its name

    @qt-parity
    Scenario: A Text too narrow for its line is elided
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool cut: label.truncated
          Text { id: label; width: 40; text: "a line much too long"; elide: Text.ElideRight }
        }
        """
      When 100 milliseconds pass
      Then the property "cut" of "root" is true

    @ljos-only
    Scenario: The same Text in Ljós's API is elided
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          objectName: "root"
          property bool cut: label.truncated
          Text { id: label; width: 40; text: "a line much too long"; elide: Text.ElideRight }
        }
        """
      When 100 milliseconds pass
      Then the property "cut" of "root" is true

  Rule: A state changes a Text by its new names

    @qt-parity
    Scenario: A state gives a Text a size, a weight and an alignment
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property int size: label.font.pixelSize
          property int weight: label.font.weight
          property int align: label.horizontalAlignment
          Text { id: label; width: 200; text: "State"; font.pixelSize: 20 }
          states: State {
            name: "big"
            PropertyChanges { target: label; font.pixelSize: 40; font.bold: true; horizontalAlignment: Text.AlignRight }
          }
          Component.onCompleted: root.state = "big"
        }
        """
      When 100 milliseconds pass
      Then the property "size" of "root" is 40
      And the property "weight" of "root" is 700
      And the property "align" of "root" is 2

    @ljos-only
    Scenario: The same state, in Ljós's names
      Given the QML:
        """qml
        import Ljos.Ui; import Ljos.Core
        Item {
          id: root
          objectName: "root"
          property int size: label.font.size
          property int weight: label.font.weight
          property int align: label.align
          Text { id: label; width: 200; text: "State"; font.size: 20 }
          states: State {
            name: "big"
            PropertyChanges { label.font.size: 40; label.font.weight: Font.Bold; label.align: Text.AlignRight }
          }
          Component.onCompleted: root.state = "big"
        }
        """
      When 100 milliseconds pass
      Then the property "size" of "root" is 40
      And the property "weight" of "root" is 700
      And the property "align" of "root" is 2

  Rule: ljos migrate writes a Text in Ljós's names, and it looks as it did

    @ljos-only @headless
    Scenario: A file with every renamed member of a Text would be lowered, and draws the same
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 200; height: 80; color: "white"
          Text {
            id: label
            width: 200
            text: "Hello"
            font.pixelSize: 20
            font.bold: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            textFormat: Text.PlainText
            elide: Text.ElideRight
            onHorizontalAlignmentChanged: console.log(label.horizontalAlignment)
          }
          Text { y: 40; text: "Bold " + label.font.pixelSize; font { pixelSize: 14; bold: label.width > 100 } }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (15 edits)
        1 files, 1 lowered, 0 kept
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    @ljos-only @headless
    Scenario: The lowered file says each member by its Ljós name
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
          width: 200; height: 80; color: "white"
          Text {
            id: label
            width: 200
            text: "Hello"
            font.pixelSize: 20
            font.bold: true
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            textFormat: Text.PlainText
            elide: Text.ElideRight
            onHorizontalAlignmentChanged: console.log(label.horizontalAlignment)
          }
          Text { y: 40; text: "Bold " + label.font.pixelSize; font { pixelSize: 14; bold: label.width > 100 } }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the file "Main.qml" is:
        """qml
        import Ljos.Ui
        Rectangle {
          width: 200; height: 80; fill: "white"
          Text {
            id: label
            width: 200
            text: "Hello"
            font.size: 20
            font.weight: Font.Bold
            align: Text.AlignHCenter
            wrap: Text.WordWrap
            format: Text.PlainText
            elide: Text.ElideRight
            onAlignChanged: console.log(label.align)
          }
          Text { y: 40; text: "Bold " + label.font.size; font { size: 14; weight: (label.width > 100) ? Font.Bold : Font.Normal } }
        }
        """

    # Ljos.Controls.Basic's Label is Qt's Basic Label, built on the toolkit's Text, not on
    # Ljos.Ui's face of it: it keeps Qt's names, and the Text beside it takes Ljós's.
    @ljos-only @headless
    Scenario: A Label keeps the names it has in Ljos.Controls.Basic
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          Label { width: 200; text: "Long words"; wrapMode: Text.Wrap; font.pixelSize: 14 }
          Text { y: 40; width: 200; text: "Long words"; wrapMode: Text.Wrap }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        Item {
          Label { width: 200; text: "Long words"; wrapMode: Text.Wrap; font.size: 14 }
          Text { y: 40; width: 200; text: "Long words"; wrap: Text.Wrap }
        }
        """

    @ljos-only @headless
    Scenario: A state's changes to a Text are written in Ljós's names
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          Text { id: label; text: "State" }
          states: State {
            name: "big"
            PropertyChanges { target: label; font.pixelSize: 40; font.bold: false; horizontalAlignment: Text.AlignRight }
          }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the file "Main.qml" is:
        """qml
        import Ljos.Ui
        Item {
          Text { id: label; text: "State" }
          states: State {
            name: "big"
            PropertyChanges { label.font.size: 40; label.font.weight: Font.Normal; label.align: Text.AlignRight }
          }
        }
        """

  Rule: A file whose Text the names cannot say is kept as written

    # Ljós's font has a weight and no boldness. A binding of `font.bold` is written as a weight;
    # a script that reads or writes it is kept, as its value is a weight's in Ljós.
    @ljos-only @headless
    Scenario: A script that reads a font's boldness keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          Text { id: label; text: "Bold"; font.bold: true }
          Text { text: label.font.bold ? "bold" : "plain" }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:4:27: `.bold` of a font is read or written, and Ljós's font has only a `weight`
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A font given both a boldness and a weight keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Text { text: "Bold"; font.bold: true; font.weight: Font.Light }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:2:27: `font.bold` and `font.weight` are both set, and are one `weight` in Ljós
        1 files, 0 lowered, 1 kept
        """

    # A point is the screen's logical DPI / 72 of a pixel, known when the file runs; the lowering
    # writes `Screen.points(n)`, and until `Screen` is in Ljos.Ui a file that says it is kept.
    @ljos-only @headless
    Scenario: A font sized in points keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Text { text: "Points"; font.pointSize: 12 }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:2:29: `font.pointSize` has no Ljós name until `Screen` is in Ljos.Ui
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A size read off something whose type is not known keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          property var face
          Text { text: "Size"; font.pixelSize: face.pixelSize }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:4:45: `.pixelSize` is read off something whose type is not known
        1 files, 0 lowered, 1 kept
        """
