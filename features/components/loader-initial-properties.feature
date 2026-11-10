# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#setSource-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#incubateObject-method
# Verified on Qt 6.11.2 and 6.12.0 (qml6, offscreen).

@qt-parity
Feature: The properties setSource gives an item it cannot take
  Loader.setSource(url, properties) sets what it can of properties on the
  new item before it completes, as incubateObject does. A property the item
  does not declare, a value it cannot take, or a read-only property is an
  error of the script that called setSource, told at the line of the call.
  The item is made all the same, with the other properties set, whether
  the Loader makes it at once or asynchronously.

  Qt names no file and a garbage line for what an asynchronous Loader
  could not set, so those scenarios check the words and not the place. Ljós
  tells the line of the call there too, as incubateObject does.

  Background:
    Given the file "Viewer.qml":
      """qml
      import QtQuick
      Item {
        objectName: "viewer"
        property string source: "none"
        property int count: 1
        readonly property int fixed: 2
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property alias asynchronous: loader.asynchronous
        Loader { id: loader; objectName: "loader" }
        function load(props) {
          loader.setSource("Viewer.qml", props)
        }
      }
      """

  Scenario Outline: A property the item does not declare is an error and the item is made, <how>
    When the property "asynchronous" of "root" is set to <asynchronous>
    And the method "load" of "root" is called with {"video": false, "source": "a.mp3", "count": 5}
    And 50 milliseconds pass
    Then the property "status" of "loader" is 1
    And the property "source" of "loader/viewer" is "a.mp3"
    And the property "count" of "loader/viewer" is 5
    And the log has a warning matching "^.*<place>: Error: Cannot assign to non-existent property .video.$"

    Examples:
      | how            | asynchronous | place    |
      | at once        | false        | [.]qml:8 |
      | asynchronously | true         |          |

  Scenario Outline: A value the property cannot take is an error and the item keeps its own, <how>
    When the property "asynchronous" of "root" is set to <asynchronous>
    And the method "load" of "root" is called with {"count": "many", "source": "a.mp3"}
    And 50 milliseconds pass
    Then the property "status" of "loader" is 1
    And the property "count" of "loader/viewer" is 1
    And the property "source" of "loader/viewer" is "a.mp3"
    And the log has a warning matching "^.*<place>: Error: Cannot assign QString to int$"

    Examples:
      | how            | asynchronous | place    |
      | at once        | false        | [.]qml:8 |
      | asynchronously | true         |          |

  Scenario Outline: A read-only property keeps its value and is an error, <how>
    When the property "asynchronous" of "root" is set to <asynchronous>
    And the method "load" of "root" is called with {"fixed": 7, "source": "a.mp3"}
    And 50 milliseconds pass
    Then the property "status" of "loader" is 1
    And the property "fixed" of "loader/viewer" is 2
    And the property "source" of "loader/viewer" is "a.mp3"
    And the log has a warning matching "^.*<place>: TypeError: Cannot assign to read-only property .fixed.$"

    Examples:
      | how            | asynchronous | place    |
      | at once        | false        | [.]qml:8 |
      | asynchronously | true         |          |
