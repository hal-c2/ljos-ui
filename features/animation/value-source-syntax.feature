# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-modifier-objects
#   https://doc.qt.io/qt-6/qtquick-statesanimations-animations.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Only "on" makes an animation a property's value source
  An animation is declared on a property with "on". Assigning one to a number property with a colon
  is an error.

  # Ljós reports a value of the wrong type as a warning and loads the rest of the file.
  @backlog
  Scenario: An animation assigned to a number property is refused
    Given the file "Wrong.qml":
      """qml
      import QtQuick
      Rectangle {
        x: NumberAnimation { from: 0; to: 100 }
      }
      """
    Then loading "Wrong.qml" is rejected

  # Ljós reports a property that does not exist as a warning and loads the rest of the file.
  @backlog
  Scenario: An animation on a property the object does not have is refused
    Given the file "Missing.qml":
      """qml
      import QtQuick
      Rectangle {
        NumberAnimation on nowhere { from: 0; to: 100 }
      }
      """
    Then loading "Missing.qml" is rejected

  Scenario: An animation can be the value of a property of an animation type
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property Animation chosen: NumberAnimation { duration: 123 }
        property int length: chosen.duration
      }
      """
    Then the property "length" of "root" is 123
