# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#retainWhileLoading-prop
#   https://doc.qt.io/qt-6/qml-qtquick-borderimage.html
#   https://doc.qt.io/qt-6/qml-qtquick-animatedimage.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An Image keeps its picture while the next one loads
  With "retainWhileLoading" the item goes on showing what it has until the
  new source is ready. It still says it is loading. Without it the item
  shows nothing in between.

  Background:
    Given a 40 by 40 image file "red.png" filled with "#ff0000"
    And a 3000 by 3000 image file "blue.png" filled with "#0000ff"

  Rule: The picture stays only when asked

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 100; height: 100
          color: "#ffffff"
          Image {
            objectName: "kept"
            width: 40; height: 40
            asynchronous: true
            retainWhileLoading: true
            source: "red.png"
          }
          Image {
            objectName: "plain"
            x: 50
            width: 40; height: 40
            asynchronous: true
            source: "red.png"
          }
        }
        """
      When 300 milliseconds pass

    Scenario: The old picture is shown while the new one loads
      When the property "source" of "kept" is set to "blue.png"
      Then the property "status" of "kept" is 2
      And the property "progress" of "kept" is 0
      And the pixel at 20, 20 of the window is "#ff0000"

    Scenario: Without it nothing is shown while the new one loads
      When the property "source" of "plain" is set to "blue.png"
      Then the property "status" of "plain" is 2
      And the pixel at 70, 20 of the window is "#ffffff"

    Scenario: The new picture takes over when it is ready
      When the property "source" of "kept" is set to "blue.png"
      And 2000 milliseconds pass
      Then the property "status" of "kept" is 1
      And the pixel at 20, 20 of the window is "#0000ff"

    Scenario: A source that fails leaves nothing
      When the property "source" of "kept" is set to "gone.png"
      When 300 milliseconds pass
      Then the property "status" of "kept" is 3
      And the property "implicitWidth" of "kept" is 0
      And the pixel at 20, 20 of the window is "#ffffff"

    Scenario: An empty source leaves nothing
      When the property "source" of "kept" is set to ""
      Then the property "status" of "kept" is 0
      And the pixel at 20, 20 of the window is "#ffffff"

  Rule: Every image item has it, off by default

    Scenario: It starts false on each of them
      Given the QML:
        """qml
        import QtQuick
        Item {
          Image { objectName: "image" }
          BorderImage { objectName: "border" }
          AnimatedImage { objectName: "animated" }
        }
        """
      Then the property "retainWhileLoading" of "image" is false
      And the property "retainWhileLoading" of "border" is false
      And the property "retainWhileLoading" of "animated" is false
