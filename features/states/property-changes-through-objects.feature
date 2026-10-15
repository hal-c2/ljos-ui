# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-aliases
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: PropertyChanges through a property that holds an object
  A change may name a property of an object the target holds, such as
  `button.opacity` where `button` is an alias of one of the target's
  children. The change is to that object's property, and leaving the
  state puts it back.

  Background:
    Given the file "Bar.qml":
      """qml
      import QtQuick
      Item {
        property alias inner: inner
        property alias pic: pic
        Rectangle { id: inner; objectName: "inner" }
        Image { id: pic; objectName: "pic"; sourceSize.height: 50 }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        Bar { id: bar }
        states: State {
          name: "on"
          PropertyChanges {
            target: bar
            inner.opacity: 0.25
            inner.enabled: false
            pic.sourceSize.width: 70
          }
        }
      }
      """

  @api-QtQuick.PropertyChanges.target
  Scenario Outline: The held object's properties change
    When the property "state" of "root" is set to "on"
    Then the property "<property>" of "<object>" is <value>
    And the log has no warning matching "non-existent property"

    Examples:
      | object | property          | value |
      | inner  | opacity           | 0.25  |
      | inner  | enabled           | false |
      | pic    | sourceSize.width  | 70    |
      | pic    | sourceSize.height | 50    |

  @api-QtQuick.PropertyChanges.target
  Scenario Outline: Leaving the state puts them back
    When the property "state" of "root" is set to "on"
    And the property "state" of "root" is set to ""
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property         | value |
      | inner  | opacity          | 1     |
      | inner  | enabled          | true  |
      | pic    | sourceSize.width | 0     |
