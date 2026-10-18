# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#sourceSize-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# The coffee demo's cup follows its item this way: the item is 0 wide
# until its layout has run, and the cup is drawn at the item's later width.

@qt-parity @api-QtQuick.Image.sourceSize
Feature: A binding on sourceSize outlives a load that asks for nothing
  A binding on "sourceSize.width" that first gives 0 asks for no size, and
  the picture is loaded at its own. The binding stays: when what it reads
  changes, the picture is loaded again at the size it gives. The size is
  held in whole pixels, as Qt's integer size holds it.

  Background:
    Given a 40 by 20 image file "pic.png" filled with "#2060a0"
    And the QML:
      """qml
      import QtQuick
      Item {
        id: box
        objectName: "box"
        width: 0
        property real asked: image.sourceSize.width
        property real drawn: image.implicitWidth
        Image {
          id: image
          objectName: "image"
          source: "pic.png"
          sourceSize.width: parent.width
        }
      }
      """

  Scenario: The picture is first loaded at its own size
    Then the property "status" of "image" is 1
    And the property "drawn" of "box" is 40

  Scenario: A later width is the size asked for
    When the property "width" of "box" is set to 10
    And 100 milliseconds pass
    Then the property "asked" of "box" is 10
    And the property "drawn" of "box" is 10

  Scenario: A width that asks for nothing again reads as set
    When the property "width" of "box" is set to 10
    And 100 milliseconds pass
    And the property "width" of "box" is set to 0
    And 100 milliseconds pass
    Then the property "asked" of "box" is 0
    And the property "drawn" of "box" is 40

  Scenario Outline: sourceSize holds whole pixels, a half rounded away from zero
    When the property "width" of "box" is set to <width>
    And 100 milliseconds pass
    Then the property "asked" of "box" is <asked>

    Examples:
      | width | asked |
      | 10.4  | 10    |
      | 10.5  | 11    |
      | 11.5  | 12    |
