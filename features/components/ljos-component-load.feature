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
          property bool none: Qt.createComponent("") === null
        }
        """
      Then the property "ready" of "root" is true
      And the property "n" of "root" is 2
      And the property "fromText" of "root" is 2
      And the property "none" of "root" is true

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
          property bool none: Component.load("") === null
        }
        """
      Then the property "ready" of "root" is true
      And the property "n" of "root" is 2
      And the property "fromText" of "root" is 2
      And the property "none" of "root" is true

  Rule: An object made from text is the one Qt's QML makes

    @qt-parity
    Scenario: Qt's object from text reads the names of the parent's file and is called inline
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property int base: 5
          property var kid: Qt.createQmlObject("import QtQuick; Item { objectName: 'made'; width: root.base * 2 }", root)
          property string caught: ""
          property bool unnamed: false
          Component.onCompleted: {
            try { Qt.createQmlObject("import QtQuick\nItem { nothing: 3 }", root) } catch (e) {
              unnamed = e.qmlErrors[0].fileName.endsWith("/inline")
            }
            try { Qt.createQmlObject("import QtQuick; Item { }", null) } catch (e) {
              caught = e.message
            }
          }
        }
        """
      Then the property "width" of "root/made" is 10
      And the property "unnamed" of "root" is true
      And the property "caught" of "root" is "Qt.createQmlObject(): Missing parent object"

    @ljos-only
    Scenario: Component.createFromText makes it so in a file of Ljós's API
      Given the QML:
        """qml
        pragma Scripts: JavaScript
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"
          property int base: 5
          property Object kid: Component.createFromText("import QtQuick; Item { objectName: 'made'; width: root.base * 2 }", root)
          property string caught: ""
          property bool unnamed: false
          Component.onCompleted: {
            try { Component.createFromText("import QtQuick\nItem { nothing: 3 }", root) } catch (e) {
              unnamed = e.qmlErrors[0].fileName.endsWith("/inline")
            }
            try { Component.createFromText("import QtQuick; Item { }", null) } catch (e) {
              caught = e.message
            }
          }
        }
        """
      Then the property "width" of "root/made" is 10
      And the property "unnamed" of "root" is true
      And the property "caught" of "root" is "Component.createFromText(): Missing parent object"

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
        }
        """
      When the files are checked
      Then the check reports:
        """
        3 files, 0 errors, 0 warnings
        """
