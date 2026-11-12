# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#completed-signal
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#setSource-method
#   https://doc.qt.io/qt-6/qml-qtqml-models-instantiator.html
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An object made in a delegate has heard its own changes when it is complete
  An object's change handlers for what its bindings and initial properties
  set while it is made run before its Component.onCompleted. That holds
  for a delegate an Instantiator or a Repeater makes, and for an item a
  Loader in such a delegate makes from the delegate's own
  Component.onCompleted, with setSource and initial properties, whether the
  Loader is active already or made active after setSource. It holds whether
  the model had its rows when the view was made or was given them after.

  Background:
    Given the file "Page.qml":
      """qml
      import QtQuick
      QtObject {
        property var held: null
      }
      """

  Scenario Outline: A Loader in a delegate makes an item, <view>, <rows>, <active>
    Given the file "Panel.qml":
      """qml
      import QtQuick
      <import>
      Item {
        id: panel
        property var source: null
        property int heard: 0
        property int heardAtCompleted: -1
        <child> {
          held: panel.source
          onHeldChanged: panel.heard++
        }
        Component.onCompleted: heardAtCompleted = heard
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: app
        objectName: "root"
        property QtObject previews: QtObject { objectName: "previews" }
        property Item panel: null
        readonly property int heardAtCompleted: panel ? panel.heardAtCompleted : -2
        readonly property int heard: panel ? panel.heard : -2
        ListModel { id: keys; <element> }
        Component.onCompleted: { <append> }
        <view> {
          model: keys
          delegate: Loader {
            active: <initial>
            Component.onCompleted: {
              setSource(Qt.resolvedUrl("Panel.qml"), { source: Qt.binding(() => app.previews) });
              active = true;
            }
            onLoaded: app.panel = item
          }
        }
      }
      """
    When 50 milliseconds pass
    Then the property "heardAtCompleted" of "root" is 1
    And the property "heard" of "root" is 1

    Examples:
      | view         | rows                | active            | element                  | append                    | initial | import | child |
      | Instantiator | rows from the start | active already    | ListElement { key: "a" } |                           | true    |        | Page  |
      | Instantiator | rows from the start | made active after | ListElement { key: "a" } |                           | false   |        | Page  |
      | Instantiator | rows given after    | active already    |                          | keys.append({ key: "a" }) | true    |        | Page  |
      | Instantiator | rows given after    | made active after |                          | keys.append({ key: "a" }) | false   |        | Page  |
      | Repeater     | rows from the start | made active after | ListElement { key: "a" } |                           | false   |        | Page  |
      | Repeater     | rows given after    | made active after |                          | keys.append({ key: "a" }) | false   |        | Page  |

    @ljos-only
    Examples: The child is of a type registered from Rust whose setter does more than store
      | view         | rows             | active            | element | append                    | initial | import          | child  |
      | Instantiator | rows given after | made active after |         | keys.append({ key: "a" }) | false   | import Fixtures | Holder |
      | Repeater     | rows given after | active already    |         | keys.append({ key: "a" }) | true    | import Fixtures | Holder |

  Scenario Outline: A delegate's child object has heard its change, <view>, <rows>
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: app
        objectName: "root"
        property QtObject previews: QtObject { objectName: "previews" }
        property int heard: 0
        property int heardAtCompleted: -1
        ListModel { id: keys; <element> }
        Component.onCompleted: { <append> }
        <view> {
          model: keys
          delegate: Item {
            id: body
            required property string key
            QtObject {
              property var held: body.key ? app.previews : null
              onHeldChanged: app.heard++
            }
            Component.onCompleted: app.heardAtCompleted = app.heard
          }
        }
      }
      """
    When 50 milliseconds pass
    Then the property "heardAtCompleted" of "root" is 1
    And the property "heard" of "root" is 1

    Examples:
      | view         | rows                | element                  | append                    |
      | Instantiator | rows from the start | ListElement { key: "a" } |                           |
      | Instantiator | rows given after    |                          | keys.append({ key: "a" }) |
      | Repeater     | rows from the start | ListElement { key: "a" } |                           |
      | Repeater     | rows given after    |                          | keys.append({ key: "a" }) |
