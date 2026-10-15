# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-aliases
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: PropertyChanges through a property that holds an object
  A change may name a property of an object that the target holds in a property, such as an alias a
  component exports for one of its children: `backButton.opacity` on a toolbar with
  `property alias backButton: back`. A member of a value of that object (`logo.sourceSize.width`)
  is reached the same way.

  Background:
    Given a 16 by 16 image file "red.png" filled with "#ff0000"
    And the file "Bar.qml":
      """qml
      import QtQuick
      Item {
        property alias back: backRect
        property alias logo: logoImage
        Rectangle { id: backRect; objectName: "backRect"; width: 10; height: 10; opacity: 1 }
        Image { id: logoImage; objectName: "logoImage"; source: "red.png"; sourceSize.width: 5; sourceSize.height: 6 }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        Bar { id: bar; objectName: "bar" }
        states: State { name: "home"
          PropertyChanges { target: bar; back.opacity: 0.5; back.enabled: false
            logo.sourceSize.width: 70; logo.sourceSize.height: 50 }
        }
      }
      """

  Scenario Outline: A member of the held object is changed in the state
    When the property "state" of "root" is set to "home"
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object    | property          | value |
      | backRect  | opacity           | 0.5   |
      | backRect  | enabled           | false |
      | logoImage | sourceSize.width  | 70    |
      | logoImage | sourceSize.height | 50    |

  Scenario: Leaving the state puts the member back
    When the property "state" of "root" is set to "home"
    And the property "state" of "root" is set to ""
    Then the property "opacity" of "backRect" is 1
    And the property "sourceSize.width" of "logoImage" is 5
