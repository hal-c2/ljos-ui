# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Loader
  A Loader makes one item from a component or from a file, and drops or
  replaces it when asked.

  Rule: An active Loader holds the item its component makes

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property int n: loader.item ? loader.item.n : -1
          property bool parented: loader.item !== null && loader.item.parent === loader
          property string events: ""
          Component {
            id: comp
            Rectangle {
              objectName: "kid"; property int n: 7; width: 30; height: 20
              Component.onCompleted: root.events += "made "
              Component.onDestruction: root.events += "gone "
            }
          }
          Loader {
            id: loader; objectName: "loader"; sourceComponent: comp
            onItemChanged: root.events += item ? "item " : "none "
            onStatusChanged: root.events += "status" + status + " "
            onLoaded: root.events += "loaded "
          }
          function unload() { loader.sourceComponent = undefined }
        }
        """

    Scenario: The item is there as soon as the Loader is
      Then the object "loader/kid" exists
      And the property "status" of "loader" is 1
      And the property "progress" of "loader" is 1
      And the property "n" of "root" is 7

    Scenario: The item is a child of the Loader
      Then the property "parented" of "root" is true

    Scenario: The item completes before the Loader says it is loaded
      Then the property "events" of "root" is "made item status1 loaded "

    Scenario: A Loader without a size takes the size of its item
      Then the property "width" of "loader" is 30
      And the property "height" of "loader" is 20
      And the property "implicitWidth" of "loader" is 30

    Scenario: A Loader without a size follows its item's size
      When the property "width" of "kid" is set to 60
      Then the property "width" of "loader" is 60

    Scenario: A Loader that is switched off destroys its item
      When the property "active" of "loader" is set to false
      Then the object "kid" does not exist
      And the property "status" of "loader" is 0
      And the property "n" of "root" is -1
      And the property "events" of "root" is "made item status1 loaded gone none status0 "

    Scenario: A Loader that is switched on again makes a new item
      When the property "active" of "loader" is set to false
      And the property "active" of "loader" is set to true
      Then the object "loader/kid" exists
      And the property "status" of "loader" is 1
      And the property "n" of "root" is 7

    Scenario: Taking the component away unloads the item and keeps the size
      When the method "unload" of "root" is called
      Then the object "kid" does not exist
      And the property "status" of "loader" is 0
      And the property "progress" of "loader" is 0
      And the property "width" of "loader" is 30

  Rule: A Loader does nothing until it is told what to load, and while it is off

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Component { id: comp; Item { objectName: "kid"; width: 30 } }
          Loader { objectName: "empty" }
          Loader { objectName: "off"; active: false; sourceComponent: comp }
        }
        """

    Scenario: A Loader with nothing to load is empty
      Then the property "status" of "empty" is 0
      And the property "progress" of "empty" is 0
      And the property "item" of "empty" is null
      And the property "source" of "empty" is ""
      And the property "width" of "empty" is 0

    Scenario: A Loader that starts off makes nothing
      Then the object "kid" does not exist
      And the property "status" of "off" is 0
      And the property "item" of "off" is null

    Scenario: Switching it on loads the item
      When the property "active" of "off" is set to true
      Then the object "off/kid" exists
      And the property "status" of "off" is 1
      And the property "width" of "off" is 30

  Rule: A Loader with a size of its own gives that size to its item

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"; width: 300; height: 200
          Component {
            id: comp
            Rectangle { implicitWidth: 11; implicitHeight: 12; width: 30; height: 20 }
          }
          Loader {
            id: sized; objectName: "sized"; width: 100; height: 50; sourceComponent: comp
            property real itemWidth: item ? item.width : -1
            property real itemHeight: item ? item.height : -1
          }
          Loader {
            objectName: "filling"; anchors.fill: parent; sourceComponent: comp
            property real itemWidth: item ? item.width : -1
            property real itemHeight: item ? item.height : -1
          }
        }
        """

    Scenario: The item is resized to the Loader
      Then the property "itemWidth" of "sized" is 100
      And the property "itemHeight" of "sized" is 50

    Scenario: The Loader keeps its size and takes the item's implicit size
      Then the property "width" of "sized" is 100
      And the property "implicitWidth" of "sized" is 11
      And the property "implicitHeight" of "sized" is 12

    Scenario: The item follows the Loader's size
      When the property "width" of "sized" is set to 80
      Then the property "itemWidth" of "sized" is 80

    Scenario: An anchored Loader counts as sized
      Then the property "itemWidth" of "filling" is 300
      And the property "itemHeight" of "filling" is 200

  Rule: A Loader loads a file named by its source

    Background:
      Given the file "Kid.qml":
        """qml
        import QtQuick
        Item { objectName: "kid"; property string tag: "kid"; property int n: 1; width: 40 }
        """
      And the file "Other.qml":
        """qml
        import QtQuick
        Item { objectName: "other"; property string tag: "other"; width: 70 }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string tag: loader.item ? loader.item.tag : ""
          property int loads: 0
          Component { id: comp; Item { objectName: "inline"; property string tag: "inline" } }
          Loader { id: loader; objectName: "loader"; source: "Kid.qml"; onLoaded: root.loads++ }
          function swap() { loader.source = "Other.qml" }
          function clear() { loader.source = "" }
          function inline() { loader.sourceComponent = comp }
          function missing() { loader.source = "Missing.qml" }
          function withProps() { loader.setSource("Kid.qml", { tag: "set", n: 9 }) }
        }
        """

    Scenario: The file's root object becomes the item
      Then the object "loader/kid" exists
      And the property "tag" of "root" is "kid"
      And the property "status" of "loader" is 1
      And the property "source" of "loader" is "Kid.qml"
      And the property "width" of "loader" is 40

    Scenario: A new source replaces the item
      When the method "swap" of "root" is called
      Then the object "kid" does not exist
      And the object "loader/other" exists
      And the property "tag" of "root" is "other"
      And the property "loads" of "root" is 2
      And the property "width" of "loader" is 70

    Scenario: An empty source unloads the item
      When the method "clear" of "root" is called
      Then the object "kid" does not exist
      And the property "status" of "loader" is 0
      And the property "progress" of "loader" is 0
      And the property "tag" of "root" is ""

    Scenario: A component takes the place of the source
      When the method "inline" of "root" is called
      Then the object "kid" does not exist
      And the property "tag" of "root" is "inline"
      And the property "source" of "loader" is ""

    Scenario: A source takes the place of the component
      When the method "inline" of "root" is called
      And the method "swap" of "root" is called
      Then the object "inline" does not exist
      And the property "tag" of "root" is "other"
      And the property "status" of "loader" is 1

    Scenario: A file that does not exist is an error
      When the method "missing" of "root" is called
      Then the property "status" of "loader" is 3
      And the property "item" of "loader" is null
      And the object "kid" does not exist
      And the log has a warning matching "Missing.qml"

    Scenario: setSource gives the new item its first property values
      When the method "withProps" of "root" is called
      Then the property "tag" of "root" is "set"
      And the property "n" of "loader/kid" is 9
      And the property "loads" of "root" is 2

    Scenario: A Loader switched off and on again keeps those values
      When the method "withProps" of "root" is called
      And the property "active" of "loader" is set to false
      And the property "active" of "loader" is set to true
      Then the property "tag" of "root" is "set"
      And the property "n" of "loader/kid" is 9

  Rule: An asynchronous Loader makes its item later

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property int loads: 0
          Component { id: comp; Item { objectName: "kid"; width: 30 } }
          Loader {
            objectName: "loader"; asynchronous: true; sourceComponent: comp
            onLoaded: root.loads++
          }
        }
        """

    Scenario: The item arrives
      Then the object "loader/kid" exists
      And the property "status" of "loader" is 1
      And the property "progress" of "loader" is 1
      And the property "loads" of "root" is 1
      And the property "width" of "loader" is 30
