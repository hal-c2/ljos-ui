# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#asynchronous-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#scale-prop
#   docs/ARCHITECTURE.md §7.1 Images
# Verified on Qt 6.11.2 (qml6, offscreen): a local picture is Ready, with
# its size, as soon as its source is set, and Qt has decoded it by then.

@ljos-only
Feature: A large local picture is decoded away from the user interface
  An Image without "asynchronous" loads a local file on the spot, as in
  Qt: its status, "sourceSize" and implicit size are there as soon as the
  source is set. For a picture of more than 512 by 512 pixels only the
  header is read then. Its pixels are decoded on another thread, at the
  size the item is drawn, and painted when they come. So the window keeps
  answering while many large pictures load. A smaller picture is decoded
  on the spot. Qt decodes every one on the spot, so the oracle has no
  frame without the pixels.

  Background:
    Given a 1024 by 768 image file "big.png" filled with "#2060a0"
    And a 40 by 20 image file "small.png" filled with "#ff0000"

  Scenario: A large picture is ready at once and painted a moment later
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 200
        Image { objectName: "picture"; width: 200; height: 150; source: "big.png" }
      }
      """
    Then the property "status" of "picture" is 1
    And the property "sourceSize.width" of "picture" is 1024
    And the property "implicitHeight" of "picture" is 768
    And no picture is kept for "picture" yet
    When 100 milliseconds pass
    Then the picture of "picture" is kept at 224 by 168 pixels
    And the pixel at 100, 75 of the window is "#2060a0"

  Scenario: A picture in a scaled-down item is decoded at the size it is drawn
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 400; height: 400
        Rectangle {
          scale: 200 / Math.max(image.sourceSize.width, image.sourceSize.height)
          width: image.width; height: image.height
          Image { id: image; objectName: "picture"; source: "big.png" }
        }
      }
      """
    When 100 milliseconds pass
    Then the property "width" of "picture" is 1024
    And the picture of "picture" is kept at 224 by 168 pixels

  Scenario: A small picture is decoded on the spot
    Given the QML:
      """qml
      import QtQuick
      Image { objectName: "picture"; source: "small.png" }
      """
    Then the property "status" of "picture" is 1
    And the picture of "picture" is kept at 40 by 20 pixels
