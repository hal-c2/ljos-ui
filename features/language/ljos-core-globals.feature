# Sources:
#   docs/DIALECT.md (§4.3 globals, §6 the compat pass)
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html
#   https://doc.qt.io/qt-6/qml-color.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# What Qt's QML has from the `Qt` global, Ljós's API has from `Ljos.Core`: constructors of the value
# types and of colours, a colour's own methods, `later`, `url` and `App`. The file as written in Qt's
# QML is the oracle, and the same file in Ljós's API, and the file lowered, are held to its numbers.
# The sum is 3 + 4 + 5 + 6 + 3 + 3. The call that comes later doubles what the handler set.

Feature: Ljos.Core: what Qt's QML has from the Qt global
  A file in Qt's QML makes a point with `Qt.point`, a colour with
  `Qt.rgba`, and asks `Qt.platform` what it runs on. A file in Ljós's API
  has the same from `Ljos.Core`, which it imports: `point(…)`,
  `rgba(…)`, a colour's own `darker`, `later(…)`, `url(…)` and
  `App.platform`. A file that does not import `Ljos.Core` has none of
  them, and a property of the same name is the property. A file whose
  member of `Qt` cannot be lowered to its home stays Qt's whole: a file
  of Ljós's API has no `Qt`.

  Rule: Ljos.Core makes what Qt makes

    @api-QtQml.Qt.alpha @api-QtQml.Qt.callLater @api-QtQml.Qt.darker @api-QtQml.Qt.platform
    @api-QtQml.Qt.point @api-QtQml.Qt.rect @api-QtQml.Qt.resolvedUrl @api-QtQml.Qt.rgba
    @api-QtQml.Qt.size @api-QtQml.Qt.tint @api-QtQml.Qt.vector3d
    @qt-parity
    Scenario: Values, colours, a later call and the platform, as written in Qt's QML
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property point p: Qt.point(3, 4)
          property size s: Qt.size(5, 6)
          property rect r: Qt.rect(1, 2, 3, 4)
          property vector3d v: Qt.vector3d(1, 2, 3)
          property real sum: p.x + p.y + s.width + s.height + r.width + v.z
          property color c: Qt.rgba(0.8, 0.4, 0.2, 1)
          property color dark: Qt.darker(c, 2)
          property color mixed: Qt.tint(c, Qt.rgba(0, 0, 1, 0.5))
          property real faint: Qt.alpha(c, 0.5).a
          property bool knowsOs: Qt.platform.os.length > 0
          property bool beside: Qt.resolvedUrl("a.png").toString().endsWith("/a.png")
          property int calls: 0
          Component.onCompleted: { Qt.callLater(() => calls = calls * 2); calls = 5 }
        }
        """
      Then the property "sum" of "root" is 24
      And the property "dark" of "root" is "#66331a"
      And the property "mixed" of "root" is "#663399"
      And the property "faint" of "root" is within 0.001 of 0.5
      And the property "knowsOs" of "root" is true
      And the property "beside" of "root" is true
      When 50 milliseconds pass
      Then the property "calls" of "root" is 10

    @ljos-only
    Scenario: The same file in Ljós's API has them from Ljos.Core
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property point p: point(3, 4)
          property size s: size(5, 6)
          property rect r: rect(1, 2, 3, 4)
          property vector3d v: vector3d(1, 2, 3)
          property real sum: p.x + p.y + s.width + s.height + r.width + v.z
          property color c: rgba(0.8, 0.4, 0.2, 1)
          property color dark: c.darker(2)
          property color mixed: c.tint(rgba(0, 0, 1, 0.5))
          property real faint: c.alpha(0.5).a
          property bool knowsOs: App.platform.os.length > 0
          property bool beside: url("a.png").toString().endsWith("/a.png")
          property int calls: 0
          Component.onCompleted: { later(() => calls = calls * 2); calls = 5 }
        }
        """
      Then the property "sum" of "root" is 24
      And the property "dark" of "root" is "#66331a"
      And the property "mixed" of "root" is "#663399"
      And the property "faint" of "root" is within 0.001 of 0.5
      And the property "knowsOs" of "root" is true
      And the property "beside" of "root" is true
      When 50 milliseconds pass
      Then the property "calls" of "root" is 10

    @ljos-only @headless
    Scenario: A file in Ljós's API that does not import Ljos.Core has no later
      Given the file "Tick.qml":
        """qml
        import Ljos.Ui
        Item {
          property int calls: 0
          function tick() { later(() => calls += 1) }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Tick.qml:4:29: error: `later` has no type: it is not in scope [ts-no-any]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    @ljos-only
    Scenario: A property of the same name is the property, not Ljos.Core's
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property int size: 3
          property int twice: size * 2
          property real across: rect(0, 0, 7, 1).width
        }
        """
      Then the property "twice" of "root" is 6
      And the property "across" of "root" is 7

  Rule: Qt's members are lowered to Ljos.Core's

    @ljos-only
    Scenario: The file as written in Qt's QML, lowered, holds the same numbers
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property point p: Qt.point(3, 4)
          property size s: Qt.size(5, 6)
          property rect r: Qt.rect(1, 2, 3, 4)
          property vector3d v: Qt.vector3d(1, 2, 3)
          property real sum: p.x + p.y + s.width + s.height + r.width + v.z
          property color c: Qt.rgba(0.8, 0.4, 0.2, 1)
          property color dark: Qt.darker(c, 2)
          property color mixed: Qt.tint(c, Qt.rgba(0, 0, 1, 0.5))
          property real faint: Qt.alpha(c, 0.5).a
          property bool knowsOs: Qt.platform.os.length > 0
          property bool beside: Qt.resolvedUrl("a.png").toString().endsWith("/a.png")
          property int calls: 0
          Component.onCompleted: { Qt.callLater(() => calls = calls * 2); calls = 5 }
        }
        """
      Then the property "sum" of "root" is 24
      And the property "dark" of "root" is "#66331a"
      And the property "mixed" of "root" is "#663399"
      And the property "faint" of "root" is within 0.001 of 0.5
      And the property "knowsOs" of "root" is true
      And the property "beside" of "root" is true
      When 50 milliseconds pass
      Then the property "calls" of "root" is 10

    @ljos-only @headless
    Scenario: A file that names Qt's makers is migrated to Ljos.Core's
      Given the file "Badge.qml":
        """qml
        import QtQuick
        Item {
          property point at: Qt.point(3, 4)
          property vector3d v: Qt.vector3d(1, 2, 3)
          property color tone: Qt.rgba(0.8, 0.4, 0.2, 1)
          property color shade: Qt.darker(tone, 2)
          property string os: Qt.platform.os
          property url icon: Qt.resolvedUrl("badge.png")
          property int calls: 0
          Component.onCompleted: Qt.callLater(() => calls += 1)
          function open(link) { Qt.openUrlExternally(link) }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Badge.qml (10 edits)
        scripts of ./Badge.qml stay JavaScript: typed, ljos check finds 1
        1 files, 1 lowered, 0 kept
        """
      And the file "Badge.qml" is:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        import Ljos.Core
        Item {
          property point at: point(3, 4)
          property vector3d v: vector3d(1, 2, 3)
          property color tone: rgba(0.8, 0.4, 0.2, 1)
          property color shade: Color.of(tone).darker(2)
          property string os: App.platform.os
          property url icon: url("badge.png")
          property int calls: 0
          Component.onCompleted: later(() => calls += 1)
          function open(link) { App.openUrlExternally(link) }
        }
        """

    @ljos-only @headless
    Scenario: A file that uses a member's Ljós name for something else stays Qt's
      Given the file "Frame.qml":
        """qml
        import QtQuick
        Item {
          property size size: Qt.size(5, 6)
          property point at: Qt.point(3, 4)
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        kept ./Frame.qml:3:23: `Qt.size` needs `size`, which a property of the file hides
        1 files, 0 lowered, 1 kept
        """
      And the file "Frame.qml" is:
        """qml
        import QtQuick
        Item {
          property size size: Qt.size(5, 6)
          property point at: Qt.point(3, 4)
        }
        """

    @ljos-only @headless
    Scenario: A file with a delegate stays Qt's, as a role could hide a member's Ljós name
      Given the file "List.qml":
        """qml
        import QtQuick
        Item { property Component delegate }
        """
      And the file "Dots.qml":
        """qml
        import QtQuick
        List {
          delegate: Item { property point at: Qt.point(1, 0) }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command's output mentions "kept ./Dots.qml:3:39: `Qt.point` needs `point`, which a role of a delegate could hide"
      And the file "Dots.qml" is:
        """qml
        import QtQuick
        List {
          delegate: Item { property point at: Qt.point(1, 0) }
        }
        """

    @ljos-only @headless
    Scenario: A file that imports a directory by path stays Qt's, as the directory may have a member's Ljós name
      Given the file "qml/App.qml":
        """qml
        import QtQuick
        Item { property string platform: "mine" }
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        import "qml"
        Item {
          property string os: Qt.platform.os
          property point at: Qt.point(3, 4)
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command's output mentions "kept ./Main.qml:4:23: `Qt.platform` needs `App`, which a type of a directory the file imports could be"
      And the file "Main.qml" is:
        """qml
        import QtQuick
        import "qml"
        Item {
          property string os: Qt.platform.os
          property point at: Qt.point(3, 4)
        }
        """

    # A `qmldir` names a directory's types, whatever its files are called.
    @ljos-only @headless
    Scenario: A directory the file imports by path whose qmldir names a member's Ljós name keeps it Qt's
      Given the file "site/Application.qml":
        """qml
        import QtQuick
        Item { property string os: "mine" }
        """
      And the file "site/qmldir":
        """
        module site
        App 1.0 Application.qml
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        import "site"
        Item {
          property string os: Qt.platform.os
        }
        """
      When the author runs "ljos migrate ."
      Then the command's output mentions "kept ./Main.qml:4:23: `Qt.platform` needs `App`, which a type of a directory the file imports could be"

    @ljos-only @headless
    Scenario: A directory or module the file imports that has no type of a member's Ljós name leaves the name free
      Given the file "parts/Swatch.qml":
        """qml
        import QtQuick
        Rectangle { width: 4; height: 4 }
        """
      And the file "Site/qmldir":
        """
        module Site
        NavLink 1.0 NavLink.qml
        """
      And the file "Site/NavLink.qml":
        """qml
        import QtQuick
        Text { text: "home" }
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        import Site
        import "parts"
        Item {
          property string os: Qt.platform.os
          property int enter: Qt.Key_Return
          Swatch {}
          NavLink {}
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command's output mentions "lowered ./Main.qml"
      And the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Site
        import "parts"
        Item {
          property string os: App.platform.os
          property int enter: Key.Return
          Swatch {}
          NavLink {}
        }
        """

    @ljos-only @headless
    Scenario: A file that gives Qt.resolvedUrl the object to resolve against stays Qt's
      Given the file "Icons.qml":
        """qml
        import QtQuick
        Item {
          property url plain: Qt.resolvedUrl("a.png")
          property url placed: Qt.resolvedUrl("b.png", this)
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        kept ./Icons.qml:4:24: `Qt.resolvedUrl` given an object to resolve against has no home in Ljós's API yet
        1 files, 0 lowered, 1 kept
        """
      And the file "Icons.qml" is:
        """qml
        import QtQuick
        Item {
          property url plain: Qt.resolvedUrl("a.png")
          property url placed: Qt.resolvedUrl("b.png", this)
        }
        """
