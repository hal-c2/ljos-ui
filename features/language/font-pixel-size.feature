# Sources:
#   https://doc.qt.io/qt-6/qml-font.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (bindings), bucket value:font:step-member-write(literal).

@qt-parity
Feature: A font's pixel size is more than zero
  A font value keeps its pixel size when it is given zero or less: there is no text of that
  size, so the write is ignored.

  @backlog
  Scenario Outline: A pixel size of <size> is ignored
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property font f: Qt.font({pixelSize: 10})
        property int size: f.pixelSize
        function shrink() { f.pixelSize = <size> }
      }
      """
    When the method "shrink" of "root" is called
    Then the property "size" of "root" is 10

    Examples:
      | size |
      | 0    |
      | -3   |
