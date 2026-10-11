# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#createComponent-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html
#   https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Qt.createComponent
  A script asks for the Component of a file by its address and makes objects
  of it. A file that is not there gives a Component in the error state that
  says why.

  Background:
    Given the file "Panel.qml":
      """qml
      import QtQuick
      Item { objectName: "panel"; property int n: 1 }
      """

  Rule: The component of a file

    Scenario: The component of a file that is there is ready
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var made: Qt.createComponent("Panel.qml")
          property bool ready: made.status === Component.Ready
          property string problem: made.errorString()
          property bool named: String(made.url).endsWith("/Panel.qml")
        }
        """
      Then the property "ready" of "root" is true
      And the property "problem" of "root" is ""
      And the property "named" of "root" is true

    Scenario: Its objects are made with the properties the script gives
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property var made: Qt.createComponent("Panel.qml")
          function make() { made.createObject(root, { objectName: "kid", n: 7 }) }
        }
        """
      When the method "make" of "root" is called
      Then the object "root/kid" exists
      And the property "n" of "root/kid" is 7

    Scenario: Without a parent the object is nobody's child
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var made: Qt.createComponent("Panel.qml")
          property var kid: null
          property bool alone: kid !== null && kid.parent === null
          function make() { kid = made.createObject(null) }
        }
        """
      When the method "make" of "root" is called
      Then the property "alone" of "root" is true
      And the object "root/panel" does not exist

    Scenario: Every call gives another component
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var one: Qt.createComponent("Panel.qml")
          property var two: Qt.createComponent("Panel.qml")
          property bool same: one === two
        }
        """
      Then the property "same" of "root" is false

    Scenario: A component asked for in the background becomes ready
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var made: Qt.createComponent("Panel.qml", Component.Asynchronous)
          property bool ready: made.status === Component.Ready
        }
        """
      When 100 milliseconds pass
      Then the property "ready" of "root" is true

    Scenario: An empty address gives no component
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool none: Qt.createComponent("") === null
        }
        """
      Then the property "none" of "root" is true

  Rule: The address is read from the file that asks

    Background:
      Given the file "parts/Panel.qml":
        """qml
        import QtQuick
        Item { objectName: "inner"; property int n: 2 }
        """
      And the file "parts/Asker.qml":
        """qml
        import QtQuick
        Item {
          property var beside: Qt.createComponent("Panel.qml")
          property var above: Qt.createComponent("../Panel.qml")
          property string picture: Qt.resolvedUrl("art/dot.png")
        }
        """

    Scenario: A relative address starts at the directory of the asking file
      Given the QML:
        """qml
        import QtQuick
        import "parts"
        Item {
          id: root
          objectName: "root"
          Asker { id: asker }
          property int beside: asker.beside.createObject(root).n
          property int above: asker.above.createObject(root).n
        }
        """
      Then the property "beside" of "root" is 2
      And the property "above" of "root" is 1

    Scenario: Qt.resolvedUrl gives the address a file means
      Given the QML:
        """qml
        import QtQuick
        import "parts"
        Item {
          objectName: "root"
          Asker { id: asker }
          property bool inParts: asker.picture.endsWith("/parts/art/dot.png")
          property bool whole: asker.picture.startsWith("file:///")
          property string kept: Qt.resolvedUrl("qrc:/art/dot.png")
        }
        """
      Then the property "inParts" of "root" is true
      And the property "whole" of "root" is true
      And the property "kept" of "root" is "qrc:/art/dot.png"

  Rule: A file that is not there

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property var made: Qt.createComponent("Missing.qml")
          property bool failed: made.status === Component.Error
          property string problem: made.errorString()
          property bool said: problem.endsWith("/Missing.qml: No such file or directory\n")
          property bool nothing: false
          function make() { nothing = made.createObject(root) === null }
        }
        """

    Scenario: The component is in the error state and says why
      Then the property "failed" of "root" is true
      And the property "said" of "root" is true

    Scenario: It makes no object
      When the method "make" of "root" is called
      Then the property "nothing" of "root" is true
      And the log has a warning matching "QQmlComponent: Component is not ready"
