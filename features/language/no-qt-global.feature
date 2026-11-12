# Sources:
#   docs/DIALECT.md (§4.3.1 no `Qt` in a file of Ljós's API, §5.1 `qt-global`, §6 the compat pass)
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html
#   https://doc.qt.io/qt-6/qml-qtqml-locale.html
#   https://doc.qt.io/qt-6/qt.html (the enums of the `Qt` namespace)
# Verified on Qt 6.11.2 (qml6, offscreen).
# A file in Qt's QML has the `Qt` global; a file in Ljós's API has none, and has each of its members
# somewhere of its own. The file as written in Qt's QML is the oracle, and the same file in Ljós's
# API, and the file lowered, are held to its numbers: `Qt.AlignLeft | Qt.AlignTop` is 1 | 32,
# `Qt.Key_0` is 0x30, `Qt.ControlModifier` is 0x04000000.

Feature: No Qt global in a file of Ljós's API
  A file in Qt's QML reaches a colour's shade, a locale, the application
  and the keys of Qt's enums through one global, `Qt`. A file that imports
  `Ljos.Ui` has no `Qt`: `ljos check` says where each member went, the
  compiled file is not handed it, and `ljos migrate` writes each member
  where it now is. A file whose every member has a home is lowered; one
  that keeps a member with none stays Qt's and says which.

  Rule: Each member of Qt has a home in Ljós's API

    @api-QtQml.Qt.Alignment @api-QtQml.Qt.MouseButtons @api-QtQml.Qt.KeyboardModifiers @api-QtQml.Qt.binding
    @api-QtQml.Qt.color @api-QtQml.Qt.colorEqual @api-QtQml.Qt.font @api-QtQml.Qt.fontFamilies
    @api-QtQml.Qt.formatDate @api-QtQml.Qt.inputMethod @api-QtQml.Qt.locale @api-QtQml.Qt.matrix4x4
    @api-QtQml.Qt.styleHints @api-QtQml.Qt.uiLanguage
    @qt-parity
    Scenario: Colours, fonts, a binding, the application, a locale and enum keys, as written in Qt's QML
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property color c: Qt.rgba(0.8, 0.4, 0.2, 1)
          property color dark: Qt.darker(c, 2)
          property bool same: Qt.colorEqual(Qt.color("red"), "#ff0000")
          property int px: Qt.font({ family: "Sans", pixelSize: 20 }).pixelSize
          property bool listed: Array.isArray(Qt.fontFamilies())
          property real m11: Qt.matrix4x4(2, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1).m11
          property string day: Qt.formatDate(new Date(2020, 0, 2), "yyyy-MM-dd")
          property string german: Qt.locale("de_DE").name
          property bool hinted: Qt.styleHints.mousePressAndHoldInterval > 0
          property bool typing: Qt.inputMethod.visible
          property int corner: Qt.AlignLeft | Qt.AlignTop
          property int digit: Qt.Key_0
          property int button: Qt.LeftButton
          property int control: Qt.ControlModifier
          property int base: 3; property int twice: 0
          property string language: Qt.uiLanguage
          Component.onCompleted: {
            twice = Qt.binding(() => base * 2)
            base = 4
            Qt.uiLanguage = "is_IS"
          }
        }
        """
      Then the property "dark" of "root" is "#66331a"
      And the property "same" of "root" is true
      And the property "px" of "root" is 20
      And the property "listed" of "root" is true
      And the property "m11" of "root" is 2
      And the property "day" of "root" is "2020-01-02"
      And the property "german" of "root" is "de_DE"
      And the property "hinted" of "root" is true
      And the property "typing" of "root" is false
      And the property "corner" of "root" is 33
      And the property "digit" of "root" is 48
      And the property "button" of "root" is 1
      And the property "control" of "root" is 67108864
      And the property "twice" of "root" is 8
      And the property "language" of "root" is "is_IS"

    @ljos-only
    Scenario: The same file in Ljós's API has them from Ljos.Core and Ljos.Ui
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property color c: rgba(0.8, 0.4, 0.2, 1)
          property color dark: Color.of(c).darker(2)
          property bool same: Color.of("red").equals("#ff0000")
          property int px: Font.of({ family: "Sans", pixelSize: 20 }).pixelSize
          property bool listed: Array.isArray(Font.families())
          property real m11: matrix4x4(2, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1).m11
          property string day: Locale.current.formatDate("2020-01-02T12:00", "yyyy-MM-dd")
          property string german: Locale.named("de_DE").name
          property bool hinted: App.hints.mousePressAndHoldInterval > 0
          property bool typing: App.inputMethod.visible
          property int corner: Align.Left | Align.Top
          property int digit: Key.Digit0; property int button: MouseButton.Left
          property int control: Modifier.Control
          property int base: 3; property int twice: 0
          property string language: App.language
          Component.onCompleted: {
            twice = bind(() => base * 2)
            base = 4
            App.language = "is_IS"
          }
        }
        """
      Then the property "dark" of "root" is "#66331a"
      And the property "same" of "root" is true
      And the property "px" of "root" is 20
      And the property "listed" of "root" is true
      And the property "m11" of "root" is 2
      And the property "day" of "root" is "2020-01-02"
      And the property "german" of "root" is "de_DE"
      And the property "hinted" of "root" is true
      And the property "typing" of "root" is false
      And the property "corner" of "root" is 33
      And the property "digit" of "root" is 48
      And the property "button" of "root" is 1
      And the property "control" of "root" is 67108864
      And the property "twice" of "root" is 8
      And the property "language" of "root" is "is_IS"

    @ljos-only @headless
    Scenario: The file in Ljós's API checks clean
      Given the file "Homes.qml":
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          property color c: rgba(0.8, 0.4, 0.2, 1)
          property color dark: Color.of(c).darker(2)
          property color light: c.lighter(1.5)
          property bool same: Color.of("red").equals("#ff0000")
          property int px: Font.of({ family: "Sans", pixelSize: 20 }).pixelSize
          property string when: "2020-01-02T12:00"
          property string day: Locale.current.formatDate(when, "yyyy-MM-dd")
          property string german: Locale.named("de_DE").name
          property string named: App.name
          property bool typing: App.inputMethod.visible
          property int corner: Align.Left | Align.Top
          property int base: 3; property int twice: 0
          function press(key: int): void { if (key === Key.Digit0) App.exit(0) }
          Component.onCompleted: {
            twice = bind(() => base * 2)
            App.language = "is_IS"
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

  Rule: A file of Ljós's API has no Qt

    @api-QtQml.Qt.createComponent
    @ljos-only @headless
    Scenario: Qt in a file of Ljós's API is an error that names the member's home
      Given the file "Main.qml":
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          property int align: Qt.AlignLeft
          property int calls: 0
          function tick(): void { Qt.callLater(() => calls += 1) }
          function make(): void { Qt.createComponent("Page.qml") }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:4:31: error: `Qt.AlignLeft` is not in a Ljós file: use `Align.Left` (Ljos.Ui) [qt-global]
        Main.qml:6:35: error: `Qt.callLater` is not in a Ljós file: use `later` (Ljos.Core) [qt-global]
        Main.qml:7:35: error: `Qt.createComponent` is not in a Ljós file, and Ljós's API has no home for it yet: write the file in Qt's QML [qt-global]
        1 files, 3 errors, 0 warnings
        ljos: 3 errors
        """

    @ljos-only @headless
    Scenario: A file of Ljós's API whose scripts are JavaScript has no Qt either
      Given the file "Loose.qml":
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        Item {
          property string os: Qt.platform.os
        }
        """
      When the files are checked
      Then the check reports:
        """
        Loose.qml:4:31: error: `Qt.platform` is not in a Ljós file: use `App.platform` (Ljos.Core) [qt-global]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    @ljos-only
    Scenario: A script of a file in Ljós's API that reaches Qt by a name it makes as it runs finds nothing
      Given the QML:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property bool seesQt: eval("typeof " + "Qt") !== "undefined"
        }
        """
      Then the property "seesQt" of "root" is false

  Rule: ljos migrate writes each member where it now is

    @api-QtQml.Qt.GlobalColor @api-QtQml.Qt.application @api-QtQml.Qt.quit
    @ljos-only @headless
    Scenario: A file whose every member of Qt has a home is lowered with the homes
      Given the file "Panel.qml":
        """qml
        import QtQuick
        Item {
          property color tone: Qt.rgba(0.8, 0.4, 0.2, 1)
          property color shade: Qt.darker(tone, 2)
          property bool same: Qt.colorEqual(tone, "#cc6633")
          property color fill: Qt.red
          property int across: Qt.AlignLeft | Qt.AlignTop
          property string day: Qt.formatDate(new Date(2020, 0, 2), "yyyy-MM-dd")
          property string german: Qt.locale("de_DE").name
          property string language: Qt.uiLanguage
          property string named: Qt.application.name
          property int twice: 0
          Component.onCompleted: twice = Qt.binding(() => across * 2)
          function press(key) { if (key === Qt.Key_0) Qt.quit() }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Panel.qml (16 edits)
        scripts of ./Panel.qml stay JavaScript: typed, ljos check finds 2
        1 files, 1 lowered, 0 kept
        """
      And the file "Panel.qml" is:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        import Ljos.Core
        Item {
          property color tone: rgba(0.8, 0.4, 0.2, 1)
          property color shade: Color.of(tone).darker(2)
          property bool same: Color.of(tone).equals("#cc6633")
          property color fill: "red"
          property int across: Align.Left | Align.Top
          property string day: Locale.current.formatDate(new Date(2020, 0, 2), "yyyy-MM-dd")
          property string german: Locale.named("de_DE").name
          property string language: App.language
          property string named: App.name
          property int twice: 0
          Component.onCompleted: twice = bind(() => across * 2)
          function press(key) { if (key === Key.Digit0) App.quit() }
        }
        """

    @ljos-only
    Scenario: The file as written in Qt's QML, lowered, runs with the homes
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property color c: Qt.rgba(0.8, 0.4, 0.2, 1)
          property color dark: Qt.darker(c, 2)
          property bool same: Qt.colorEqual(Qt.color("red"), "#ff0000")
          property color fill: Qt.red
          property int px: Qt.font({ family: "Sans", pixelSize: 20 }).pixelSize
          property real m11: Qt.matrix4x4(2, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1).m11
          property string day: Qt.formatDate(new Date(2020, 0, 2), "yyyy-MM-dd")
          property string german: Qt.locale("de_DE").name
          property int corner: Qt.AlignLeft | Qt.AlignTop
          property int digit: Qt.Key_0
          property int control: Qt.ControlModifier
          property int base: 3
          property int twice: 0
          property string language: Qt.uiLanguage
          Component.onCompleted: {
            twice = Qt.binding(() => base * 2)
            base = 4
            Qt.uiLanguage = "is_IS"
          }
        }
        """
      Then the property "dark" of "root" is "#66331a"
      And the property "same" of "root" is true
      And the property "fill" of "root" is "#ff0000"
      And the property "px" of "root" is 20
      And the property "m11" of "root" is 2
      And the property "day" of "root" is "2020-01-02"
      And the property "german" of "root" is "de_DE"
      And the property "corner" of "root" is 33
      And the property "digit" of "root" is 48
      And the property "control" of "root" is 67108864
      And the property "twice" of "root" is 8
      And the property "language" of "root" is "is_IS"

    @ljos-only @headless
    Scenario: A file that keeps a member with no home stays Qt's and says which
      Given the file "Maker.qml":
        """qml
        import QtQuick
        Item {
          property point at: Qt.point(3, 4)
          function make() { return Qt.createComponent("Page.qml") }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        kept ./Maker.qml:4:28: `Qt.createComponent` has no home in Ljós's API yet
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A file whose own property hides a home stays Qt's and says which
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          property int bind: 2
          property int twice: 0
          Component.onCompleted: twice = Qt.binding(() => bind * 2)
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        kept ./Main.qml:5:34: `Qt.binding` needs `bind`, which a property of the file hides
        1 files, 0 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A colour named by Qt that does not go to a colour stays Qt's
      Given the file "Swatch.qml":
        """qml
        import QtQuick
        Item {
          property int which: Qt.red
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        kept ./Swatch.qml:3:23: `Qt.red` is a number here: Ljós's API names a colour only where it goes to a `color`
        1 files, 0 lowered, 1 kept
        """
