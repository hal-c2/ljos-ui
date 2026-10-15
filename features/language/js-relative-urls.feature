# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-resources.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#resolvedUrl-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#createComponent-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Relative addresses written in a script file
  An address written in a script file is resolved against the script's own
  file, whether it is a library or not, and whichever file calls it.

  Background:
    Given the file "sub/Chip.qml":
      """qml
      import QtQuick
      Item { property string tag: "chip" }
      """

  Rule: A library resolves against its own file

    Scenario: A library resolves an address while it loads
      Given the file "sub/lib.js":
        """
        .pragma library
        var at = String(Qt.resolvedUrl("x"))
        """
      And the QML:
        """qml
        import QtQuick
        import "sub/lib.js" as Lib
        Item {
          objectName: "root"
          readonly property string here: String(Qt.resolvedUrl("sub/x"))
          property bool same: Lib.at === here
        }
        """
      Then the property "same" of "root" is true

    Scenario: A library resolves an address inside a function
      Given the file "sub/lib.js":
        """
        .pragma library
        function at() { return String(Qt.resolvedUrl("x")) }
        """
      And the QML:
        """qml
        import QtQuick
        import "sub/lib.js" as Lib
        Item {
          objectName: "root"
          readonly property string here: String(Qt.resolvedUrl("sub/x"))
          property bool same: Lib.at() === here
        }
        """
      Then the property "same" of "root" is true

    Scenario: A library makes a component from its own directory while it loads
      Given the file "sub/lib.js":
        """
        .pragma library
        var chip = Qt.createComponent("Chip.qml")
        function status() { return chip.status }
        function make() { return chip.createObject(null) }
        """
      And the QML:
        """qml
        import QtQuick
        import "sub/lib.js" as Lib
        Item {
          objectName: "root"
          property bool ready: Lib.status() === Component.Ready
          property string tag: Lib.make().tag
        }
        """
      Then the property "ready" of "root" is true
      And the property "tag" of "root" is "chip"

    Scenario: A library called from a file in another directory still uses its own
      Given the file "sub/lib.js":
        """
        .pragma library
        function at() { return String(Qt.resolvedUrl("x")) }
        """
      And the file "other/Caller.qml":
        """qml
        import QtQuick
        import "../sub/lib.js" as Lib
        Item { property string got: Lib.at() }
        """
      And the QML:
        """qml
        import QtQuick
        import "other"
        Item {
          objectName: "root"
          readonly property string here: String(Qt.resolvedUrl("sub/x"))
          Caller { id: c }
          property bool same: c.got === here
        }
        """
      Then the property "same" of "root" is true

  Rule: A script that is not a library resolves against its own file too

    Scenario: The importer is in another directory than the script
      Given the file "sub/plain.js":
        """
        var top = String(Qt.resolvedUrl("x"))
        function at() { return String(Qt.resolvedUrl("x")) }
        """
      And the file "other/Caller.qml":
        """qml
        import QtQuick
        import "../sub/plain.js" as P
        Item {
          readonly property string here: String(Qt.resolvedUrl("../sub/x"))
          property bool topSame: P.top === here
          property bool fnSame: P.at() === here
        }
        """
      And the QML:
        """qml
        import QtQuick
        import "other"
        Item {
          objectName: "root"
          Caller { id: c }
          property bool topSame: c.topSame
          property bool fnSame: c.fnSame
        }
        """
      Then the property "topSame" of "root" is true
      And the property "fnSame" of "root" is true
