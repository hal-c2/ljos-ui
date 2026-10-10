# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#fillMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#paintedHeight-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Image fill mode
  "fillMode" says what happens when the Image is not the size of its
  picture. "paintedWidth" and "paintedHeight" say how large the picture is
  drawn.

  Background:
    Given a 40 by 20 image file "red.png" filled with "#ff0000"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 200
        property var modes: [Image.Stretch, Image.PreserveAspectFit,
          Image.PreserveAspectCrop, Image.Tile, Image.TileVertically,
          Image.TileHorizontally, Image.Pad]
        property var sides: [Image.AlignLeft, Image.AlignRight,
          Image.AlignHCenter, Image.AlignTop, Image.AlignBottom,
          Image.AlignVCenter]
        property var states: [Image.Null, Image.Ready, Image.Loading,
          Image.Error]
        Image { objectName: "picture"; source: "red.png" }
        Image { objectName: "empty" }
      }
      """

  Scenario: The names of the fill modes, alignments and states
    Then the property "modes" of "root" is [0, 1, 2, 3, 4, 5, 6]
    And the property "sides" of "root" is [1, 2, 4, 32, 64, 128]
    And the property "states" of "root" is [0, 1, 2, 3]

  Scenario Outline: How large the picture is drawn
    When the property "fillMode" of "picture" is set to <mode>
    And the property "width" of "picture" is set to <w>
    And the property "height" of "picture" is set to <h>
    Then the property "paintedWidth" of "picture" is <painted w>
    And the property "paintedHeight" of "picture" is <painted h>

    Examples:
      | mode | w   | h   | painted w | painted h |
      | 0    | 100 | 100 | 100       | 100       |
      | 1    | 100 | 100 | 100       | 50        |
      | 1    | 30  | 100 | 30        | 15        |
      | 2    | 100 | 100 | 200       | 100       |
      | 2    | 100 | 20  | 100       | 50        |
      | 3    | 100 | 100 | 100       | 100       |
      | 4    | 100 | 100 | 100       | 100       |
      | 5    | 100 | 100 | 100       | 100       |
      | 6    | 100 | 100 | 40        | 20        |

  Scenario: The picture's own size is drawn while the Image has no other size
    Then the property "paintedWidth" of "picture" is 40
    And the property "paintedHeight" of "picture" is 20

  Scenario: A picture kept in shape follows a width alone
    When the property "fillMode" of "picture" is set to 1
    And the property "width" of "picture" is set to 100
    Then the property "height" of "picture" is 20
    And the property "paintedWidth" of "picture" is 40
    And the property "paintedHeight" of "picture" is 20

  Scenario Outline: An Image without a picture draws nothing it could measure
    When the property "fillMode" of "empty" is set to <mode>
    And the property "width" of "empty" is set to 100
    And the property "height" of "empty" is set to 50
    Then the property "paintedWidth" of "empty" is <painted w>
    And the property "paintedHeight" of "empty" is <painted h>

    Examples:
      | mode | painted w | painted h |
      | 0    | 100       | 50        |
      | 1    | 0         | 0         |
      | 3    | 100       | 50        |
      | 6    | 0         | 0         |

  Scenario: The fill mode does not change the Image's own size
    When the property "fillMode" of "picture" is set to 2
    Then the property "implicitWidth" of "picture" is 40
    And the property "width" of "picture" is 40
