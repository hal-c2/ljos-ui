# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-borderimage.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: BorderImage
  A BorderImage cuts its picture into nine parts with four border lines.
  The corners keep their size; the edges and the middle are stretched or
  repeated to fill the item.

  Background:
    Given a 30 by 30 image file "frame.png" filled with "#804020"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 200
        property var modes: [BorderImage.Stretch, BorderImage.Repeat,
          BorderImage.Round]
        BorderImage {
          objectName: "frame"
          source: "frame.png"
          width: 120; height: 80
          border { left: 5; top: 6; right: 7; bottom: 8 }
        }
        BorderImage { objectName: "plain"; source: "frame.png" }
        BorderImage { objectName: "empty" }
      }
      """

  Scenario Outline: Defaults
    Then the property "<property>" of "empty" is <value>

    Examples:
      | property           | value |
      | status             | 0     |
      | progress           | 0     |
      | border.left        | 0     |
      | border.top         | 0     |
      | border.right       | 0     |
      | border.bottom      | 0     |
      | horizontalTileMode | 0     |
      | verticalTileMode   | 0     |
      | asynchronous       | false |
      | cache              | true  |
      | mirror             | false |
      | smooth             | true  |
      | implicitWidth      | 0     |

  Scenario: The names of the tile modes
    Then the property "modes" of "root" is [0, 1, 2]

  Scenario: The border lines are a grouped property
    Then the property "border.left" of "frame" is 5
    And the property "border.top" of "frame" is 6
    And the property "border.right" of "frame" is 7
    And the property "border.bottom" of "frame" is 8

  Scenario: The picture loads at once and gives the implicit size
    Then the property "status" of "plain" is 1
    And the property "progress" of "plain" is 1
    And the property "implicitWidth" of "plain" is 30
    And the property "implicitHeight" of "plain" is 30
    And the property "width" of "plain" is 30
    And the property "sourceSize.width" of "plain" is 30

  Scenario: A size that was set wins over the picture's
    Then the property "width" of "frame" is 120
    And the property "height" of "frame" is 80
    And the property "implicitWidth" of "frame" is 30

  Scenario: The tile modes can be changed
    When the property "horizontalTileMode" of "frame" is set to 1
    And the property "verticalTileMode" of "frame" is set to 2
    Then the property "horizontalTileMode" of "frame" is 1
    And the property "verticalTileMode" of "frame" is 2
    And the property "status" of "frame" is 1

  Scenario: A file that is not there is an error and a warning
    When the property "source" of "frame" is set to "missing.png"
    Then the property "status" of "frame" is 3
    And the property "progress" of "frame" is 0
    And the log has a warning matching "QML BorderImage: Cannot open: file:///.*missing\.png$"

  Scenario: An empty source leaves nothing
    When the property "source" of "plain" is set to ""
    Then the property "status" of "plain" is 0
    And the property "implicitWidth" of "plain" is 0
