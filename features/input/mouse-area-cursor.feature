# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#cursorShape-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The cursor shape a MouseArea asks for
  An area names the shape the cursor takes while the pointer is over it.
  Neither backend can show the operating system's cursor headlessly, so
  what is pinned here is the shape the area holds: what it starts as,
  what taking it back with `undefined` leaves, and that disabling or
  hiding the area leaves the shape it holds as it was (the pointer
  simply stops being over an area that asks for it).

  Scenario: An area starts with the arrow and takes the shape it is given
    Given the QML:
      """qml
      import QtQuick
      MouseArea {
        objectName: "area"
        width: 100; height: 100
      }
      """
    Then the property "cursorShape" of "area" is 0
    When the property "cursorShape" of "area" is set to 13
    Then the property "cursorShape" of "area" is 13

  Scenario: A shape given in QML can be taken back with undefined
    Given the QML:
      """qml
      import QtQuick
      MouseArea {
        objectName: "area"
        width: 100; height: 100
        cursorShape: Qt.PointingHandCursor
        function clear() { cursorShape = undefined }
      }
      """
    Then the property "cursorShape" of "area" is 13
    When the method "clear" of "area" is called
    Then the property "cursorShape" of "area" is 0

  Scenario: Disabling or hiding an area leaves the shape it holds as it was
    Given the QML:
      """qml
      import QtQuick
      MouseArea {
        objectName: "area"
        width: 100; height: 100
        cursorShape: Qt.IBeamCursor
      }
      """
    When the property "enabled" of "area" is set to false
    Then the property "cursorShape" of "area" is 4
    When the property "visible" of "area" is set to false
    Then the property "cursorShape" of "area" is 4
