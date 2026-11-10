# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#setSource-method
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#asynchronous-prop
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#item-prop
#   https://doc.qt.io/qt-6/qml-qtqml-connections.html#target-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The item setSource makes
  setSource(url, properties) makes one item from url, with the properties
  set before it completes, whether the Loader makes it at once or
  asynchronously. No other item is made and left alive next to it to act
  on its own, so a Connections whose target is the Loader's item handles
  the signals the item that does its work emits.

  Background:
    Given the file "Viewer.qml":
      """qml
      import QtQuick
      Item {
        id: viewer
        objectName: "viewer"
        property url source
        property int own: 0
        signal failed(url source)
        onFailed: own++
        Component.onCompleted: parent.made++
        Timer { interval: 100; running: true; onTriggered: viewer.failed(viewer.source) }
      }
      """
    And the file "Host.qml":
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property alias asynchronous: loader.asynchronous
        property int items: 0
        property int loaded: 0
        property int heard: 0
        property string last: ""
        function load(source) { loader.setSource("Viewer.qml", { source: source }) }
        Loader {
          id: loader
          objectName: "loader"
          property int made: 0
          onItemChanged: if (item) root.items++
          onLoaded: root.loaded++
          Component.onCompleted: setSource("Viewer.qml", { source: "a.mp3" })
        }
        Connections {
          target: loader.item
          function onFailed(source) { root.heard++; root.last = String(source).split("/").pop() }
        }
      }
      """

  Scenario Outline: setSource makes one item, <how>
    Given the QML:
      """qml
      Host { asynchronous: <asynchronous> }
      """
    When 50 milliseconds pass
    Then the property "made" of "loader" is 1
    And the property "items" of "root" is 1
    And the property "loaded" of "root" is 1

    Examples:
      | how            | asynchronous |
      | at once        | false        |
      | asynchronously | true         |

  Scenario Outline: A signal the item emits reaches the Connections on the Loader's item, <how>
    Given the QML:
      """qml
      Host { asynchronous: <asynchronous> }
      """
    When 300 milliseconds pass
    Then the property "own" of "loader/viewer" is 1
    And the property "heard" of "root" is 1
    And the property "last" of "root" is "a.mp3"

    Examples:
      | how            | asynchronous |
      | at once        | false        |
      | asynchronously | true         |

  Scenario Outline: setSource with the source already loaded makes a new item, <how>
    Given the QML:
      """qml
      Host { asynchronous: <asynchronous> }
      """
    When 50 milliseconds pass
    And the method "load" of "root" is called with "a.mp3"
    And 50 milliseconds pass
    Then the property "made" of "loader" is 2
    And the property "items" of "root" is 2
    And the property "loaded" of "root" is 2

    Examples:
      | how            | asynchronous |
      | at once        | false        |
      | asynchronously | true         |

  Scenario Outline: setSource with another source makes one new item, <how>
    Given the QML:
      """qml
      Host { asynchronous: <asynchronous> }
      """
    When 50 milliseconds pass
    And the method "load" of "root" is called with "b.mp3"
    And 300 milliseconds pass
    Then the property "made" of "loader" is 2
    And the property "items" of "root" is 2
    And the property "loaded" of "root" is 2
    And the property "heard" of "root" is 1
    And the property "last" of "root" is "b.mp3"

    Examples:
      | how            | asynchronous |
      | at once        | false        |
      | asynchronously | true         |
