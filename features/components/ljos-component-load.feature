# Sources:
#   docs/DIALECT.md §4.11 (a component type is a value), §6.3 (what the compat pass lowers)
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#createComponent-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#createQmlObject-method
#   https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Core` is Ljós's own module, so its files are `@ljos-only`. Each holds the numbers of the Qt
# file above it, which runs on both. A relative address is read from the file the script is written
# in, not from the file that calls it: `dyn/Maker.qml` asking for "Part.qml" gets `dyn/Part.qml`
# though `Main.qml`, beside another `Part.qml`, calls it (qml6).

Feature: A component made from an address or from QML text, in Ljós's API
  A script of Qt's QML makes objects as it runs through `Qt`: the
  component of a file by its address, `Qt.createComponent`, and an
  object from QML it holds as text, `Qt.createQmlObject`. A file of
  Ljós's API has no `Qt`; the component type has both:
  `Component.load(address)` and `Component.createFromText(text, parent,
  place)`. Each reads an address from the file its script is written in,
  as Qt's do, and `ljos migrate` writes the one for the other.

  Background:
    Given the file "Part.qml":
      """qml
      import QtQuick
      Item { objectName: "outer"; property int n: 1 }
      """
    And the file "dyn/Part.qml":
      """qml
      import QtQuick
      Item { objectName: "inner"; property int n: 2 }
      """

  Rule: An address is read from the file the script is written in

    @qt-parity
    Scenario: Qt's makers read an address from the file whose script asks
      Given the file "dyn/Maker.qml":
        """qml
        import QtQuick
        Item {
          function load() { return Qt.createComponent("Part.qml") }
          function fromText(parent) { return Qt.createQmlObject("import QtQuick; Part { }", parent) }
        }
        """
      And the QML:
        """qml
        import QtQuick
        import "dyn" as D
        Item {
          id: root; objectName: "root"
          D.Maker { id: maker }
          property var made: maker.load()
          property bool ready: made.status === Component.Ready
          property int n: made.createObject(root).n
          property int fromText: maker.fromText(root).n
        }
        """
      Then the property "ready" of "root" is true
      And the property "n" of "root" is 2
      And the property "fromText" of "root" is 2

    @ljos-only
    Scenario: Component.load and Component.createFromText read it from the file whose script asks
      Given the file "dyn/Maker.qml":
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          function load(): Component { return Component.load("Part.qml") }
          function fromText(parent: Item): Object {
            return Component.createFromText("import QtQuick; Part { }", parent)
          }
        }
        """
      And the QML:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        import Ljos.Core
        import "dyn" as D
        Item {
          id: root; objectName: "root"
          D.Maker { id: maker }
          property Component made: maker.load()
          property bool ready: made.status === Component.Ready
          property int n: made.createObject(root).n
          property int fromText: maker.fromText(root).n
        }
        """
      Then the property "ready" of "root" is true
      And the property "n" of "root" is 2
      And the property "fromText" of "root" is 2

    @qt-parity
    Scenario: Qt's component of an empty address is null
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property bool none: Qt.createComponent("") === null
        }
        """
      Then the property "none" of "root" is true

    @ljos-only
    Scenario: Component.load of an empty address is null
      Given the QML:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property bool none: Component.load("") === null
        }
        """
      Then the property "none" of "root" is true

  Rule: An object made from text is the one Qt's QML makes

    @qt-parity
    Scenario: Qt's object from text reads the names of the parent's file
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int base: 5
          property var kid: Qt.createQmlObject("import QtQuick; Item { objectName: 'made'; width: root.base * 2 }", root)
        }
        """
      Then the property "width" of "root/made" is 10

    @ljos-only
    Scenario: Component.createFromText reads the names of the parent's file
      Given the QML:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property int base: 5
          Component.onCompleted: Component.createFromText("import QtQuick; Item { objectName: 'made'; width: root.base * 2 }", root)
        }
        """
      Then the property "width" of "root/made" is 10

    @qt-parity
    Scenario: Qt's object from text that does not compile is told of in a file called inline
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property bool unnamed: false
          Component.onCompleted: {
            try { Qt.createQmlObject("import QtQuick\nItem { nothing: 3 }", root) } catch (e) {
              unnamed = e.qmlErrors[0].fileName.endsWith("/inline")
            }
          }
        }
        """
      Then the property "unnamed" of "root" is true

    @ljos-only
    Scenario: Component.createFromText of text that does not compile is told of in a file called inline
      Given the QML:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property bool unnamed: false
          Component.onCompleted: {
            try { Component.createFromText("import QtQuick\nItem { nothing: 3 }", root) } catch (e) {
              unnamed = e.qmlErrors[0].fileName.endsWith("/inline")
            }
          }
        }
        """
      Then the property "unnamed" of "root" is true

    @qt-parity
    Scenario: Qt's object from text without a parent throws
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string caught: ""
          Component.onCompleted: {
            try { Qt.createQmlObject("import QtQuick; Item { }", null) } catch (e) {
              caught = e.message
            }
          }
        }
        """
      Then the property "caught" of "root" is "Qt.createQmlObject(): Missing parent object"

    @ljos-only
    Scenario: Component.createFromText without a parent throws
      Given the QML:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property string caught: ""
          Component.onCompleted: {
            try { Component.createFromText("import QtQuick; Item { }", null) } catch (e) {
              caught = e.message
            }
          }
        }
        """
      Then the property "caught" of "root" is "Component.createFromText(): Missing parent object"

    @ljos-only @headless
    Scenario: The file in Ljós's API checks clean
      Given the file "Maker.qml":
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root
          function load(address: string): bool {
            const made = Component.load(address)
            return made.status === Component.Ready
          }
          function fromText(text: string): Object { return Component.createFromText(text, root, "dyn/at") }
          function fromTextAt(text: string, place?: string): Object { return Component.createFromText(text, root, place) }
        }
        """
      When the files are checked
      Then the check reports:
        """
        3 files, 0 errors, 0 warnings
        """

  Rule: A file of Qt's QML has Qt's makers, not the component type's

    @qt-parity
    Scenario: The component type in a file of Qt's QML has no load and no createFromText
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string load: typeof Component.load
          property string fromText: typeof Component.createFromText
        }
        """
      Then the property "load" of "root" is "undefined"
      And the property "fromText" of "root" is "undefined"

  Rule: An object the component type made is destroyed as any other

    @ljos-only
    Scenario: An object Component.load made is gone once a file of Qt's QML destroys it
      Given the file "dyn/Maker.qml":
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        import Ljos.Core
        Item {
          function make(parent) { return Component.load("Part.qml").createObject(parent) }
          function fromText(parent) {
            return Component.createFromText("import QtQuick; Part { }", parent)
          }
        }
        """
      And the QML:
        """qml
        import QtQuick
        import "dyn" as D
        Item {
          id: root; objectName: "root"
          property int before: -1
          property int after: -1
          D.Maker { id: maker }
          Timer { id: later; interval: 0; onTriggered: after = root.children.length - 1 }
          Component.onCompleted: {
            const made = maker.make(root)
            const text = maker.fromText(root)
            before = root.children.length - 1
            made.destroy()
            text.destroy()
            later.start()
          }
        }
        """
      When 100 milliseconds pass
      Then the property "before" of "root" is 2
      And the property "after" of "root" is 0

  Rule: ljos migrate writes Qt's makers as the component type's

    @api-QtQml.Qt.createComponent @api-QtQml.Qt.createQmlObject
    @ljos-only @headless
    Scenario: A file that makes objects through Qt is lowered to Component's makers
      Given the file "Host.qml":
        """qml
        import QtQuick
        Item {
          id: host
          property string url: "Part.qml"
          function load() {
            const component = Qt.createComponent(url)
            return component.status === Component.Ready ? component.createObject(host) : null
          }
          function fromText(text, file) { return Qt.createQmlObject(text, host, file) }
          Component.onCompleted: load()
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Host.qml (3 edits)
        scripts of ./Host.qml stay JavaScript: typed, ljos check finds 3
        lowered ./Part.qml (1 edits)
        lowered ./dyn/Part.qml (1 edits)
        3 files, 3 lowered, 0 kept
        """
      And the file "Host.qml" is:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: host
          property string url: "Part.qml"
          function load() {
            const component = Component.load(url)
            return component.status === Component.Ready ? component.createObject(host) : null
          }
          function fromText(text, file) { return Component.createFromText(text, host, file) }
          Component.onCompleted: load()
        }
        """

    @ljos-only
    Scenario: The file as written in Qt's QML, lowered, reads the address from its own file
      Given Qt QML is lowered to the Ljós API
      And the file "dyn/Maker.qml":
        """qml
        import QtQuick
        Item {
          function load() { return Qt.createComponent("Part.qml") }
          function fromText(parent) { return Qt.createQmlObject("import QtQuick; Part { }", parent) }
        }
        """
      And the QML:
        """qml
        import QtQuick
        import "dyn" as D
        Item {
          id: root; objectName: "root"
          D.Maker { id: maker }
          property Component made: maker.load()
          property bool ready: made.status === Component.Ready
          property int n: made.createObject(root).n
          property int fromText: maker.fromText(root).n
        }
        """
      Then the property "ready" of "root" is true
      And the property "n" of "root" is 2
      And the property "fromText" of "root" is 2
