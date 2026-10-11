# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#createQmlObject-method
#   https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Qt.createQmlObject
  A script makes an object from QML it holds as text. The object is a child
  of the parent the script names and reads the names that parent's file
  reads. Text that is not a QML document is an error the script can catch.

  Background:
    Given the file "Panel.qml":
      """qml
      import QtQuick
      Item { objectName: "panel"; property int n: 1 }
      """
    And the file "dyn/Panel.qml":
      """qml
      import QtQuick
      Item { objectName: "other"; property int n: 2 }
      """

  Rule: The text is a QML document

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property int base: 5
          property var kid: null
          property int n: kid ? kid.n : 0
          property int kids: children.length
          function make(text) { kid = Qt.createQmlObject(text, root) }
          function makeAt(text, where) { kid = Qt.createQmlObject(text, root, where) }
        }
        """

    Scenario: The object is made and is a child of the parent
      When the method "make" of "root" is called with "import QtQuick; Item { objectName: 'made'; width: 30 }"
      Then the property "width" of "root/made" is 30
      And the property "kids" of "root" is 1

    Scenario: Its bindings read the names of the parent's file
      When the method "make" of "root" is called with "import QtQuick; Item { objectName: 'made'; width: root.base * 2; height: base }"
      And the property "base" of "root" is set to 7
      Then the property "width" of "root/made" is 14
      And the property "height" of "root/made" is 7

    Scenario: A type of a file beside the asking file is found
      When the method "make" of "root" is called with "import QtQuick; Panel { }"
      Then the property "n" of "root" is 1

    Scenario: The place the script names for the text is where its types are looked for
      When the method "makeAt" of "root" is called with "import QtQuick; Panel { }", "dyn/snippet"
      Then the property "n" of "root" is 2

    Scenario: An empty text makes nothing
      When the method "make" of "root" is called with ""
      Then the property "kids" of "root" is 0
      And the log has no warnings

  Rule: Text that cannot be made is an error for the script

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string caught: ""
          property var first: null
          property int line: first ? first.lineNumber : 0
          property bool inBad: first !== null && first.fileName.endsWith("/dyn/bad")
          property bool unnamed: first !== null && first.fileName.endsWith("/inline")
          property bool located: /object: \n    file:.*\/dyn\/bad:2:\d+: /.test(caught)
          property bool started: caught.startsWith("Error: Qt.createQmlObject(): failed to create object: ")
          function attempt(f) {
            try { f() } catch (e) {
              caught = e.name + ": " + e.message
              first = e.qmlErrors ? e.qmlErrors[0] : null
            }
          }
          function make(text) { attempt(() => Qt.createQmlObject(text, root)) }
          function makeAt(text, where) { attempt(() => Qt.createQmlObject(text, root, where)) }
          function makeAlone(text) { attempt(() => Qt.createQmlObject(text, null)) }
        }
        """

    Scenario: A property the type does not have is an error that says where
      When the method "makeAt" of "root" is called with "import QtQuick\nItem { nothing: 3 }", "dyn/bad"
      Then the property "started" of "root" is true
      And the property "located" of "root" is true
      And the property "line" of "root" is 2
      And the property "inBad" of "root" is true

    Scenario: Text with no place named is called inline
      When the method "make" of "root" is called with "import QtQuick\nItem { nothing: 3 }"
      Then the property "unnamed" of "root" is true
      And the property "line" of "root" is 2

    Scenario: Text that does not parse
      When the method "makeAt" of "root" is called with "import QtQuick\nItem { ", "dyn/bad"
      Then the property "started" of "root" is true
      And the property "inBad" of "root" is true

    Scenario: A required property the text does not set
      When the method "makeAt" of "root" is called with "import QtQuick\nItem { required property int k }", "dyn/bad"
      Then the property "started" of "root" is true
      And the property "located" of "root" is true

    Scenario: No parent is an error
      When the method "makeAlone" of "root" is called with "import QtQuick; Item { }"
      Then the property "caught" of "root" is "Error: Qt.createQmlObject(): Missing parent object"

    Scenario: A name the text reads that nothing has is not an error
      When the method "makeAt" of "root" is called with "import QtQuick\nItem { objectName: 'made'; width: nope.x }", "dyn/bad"
      Then the property "caught" of "root" is ""
      And the property "width" of "root/made" is 0

    # Waits for runtime/unresolved-name.feature: a name that nothing declares
    # reads as undefined here, so the warning is a TypeError about its member.
    @backlog
    Scenario: A name the text reads that nothing has is a warning at its place in the text
      When the method "makeAt" of "root" is called with "import QtQuick\nItem { objectName: 'made'; width: nope.x }", "dyn/bad"
      Then the log has a warning matching "dyn/bad:2:\d+: ReferenceError: nope is not defined"
