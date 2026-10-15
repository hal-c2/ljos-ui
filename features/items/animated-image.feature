# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-animatedimage.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: AnimatedImage
  An AnimatedImage plays the frames of an animated GIF, each for as long as
  the file says, over and over. It can be paused, stopped, sped up and
  turned to a frame of the author's choosing.

  Background:
    Given a 30 by 20 animated image file "lights.gif" of frames "#ff0000, #00ff00, #0000ff" each shown for 200 milliseconds
    And a 30 by 20 animated image file "still.gif" of frames "#ff0000" each shown for 200 milliseconds

  Scenario: A local animation is there at once, on its first frame and playing
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string seen: "?"
        AnimatedImage {
          id: lights; objectName: "lights"; source: "lights.gif"
        }
        Component.onCompleted: seen = [lights.status, lights.frameCount,
          lights.currentFrame, lights.playing, lights.implicitWidth].join(" ")
      }
      """
    Then the property "seen" of "root" is "1 3 0 true 30"
    And the property "sourceSize.height" of "lights" is 20

  Scenario: The frames follow each other at the pace the file sets
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 30; height: 20
        AnimatedImage { objectName: "lights"; source: "lights.gif" }
      }
      """
    Then the pixel at 15, 10 of the window is "#ff0000"
    When 300 milliseconds pass
    Then the property "currentFrame" of "lights" is 1
    And the pixel at 15, 10 of the window is "#00ff00"
    When 200 milliseconds pass
    Then the property "currentFrame" of "lights" is 2
    When 200 milliseconds pass
    Then the property "currentFrame" of "lights" is 0

  Scenario: Each change of frame is announced
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property var frames: []
        AnimatedImage {
          source: "lights.gif"
          onFrameChanged: root.frames = root.frames.concat([currentFrame])
        }
      }
      """
    When 500 milliseconds pass
    Then the property "frames" of "root" is [0, 1, 2]

  Scenario: Twice the speed shows each frame half as long
    Given the QML:
      """qml
      import QtQuick
      Item {
        AnimatedImage { objectName: "fast"; source: "lights.gif"; speed: 2 }
      }
      """
    When 250 milliseconds pass
    Then the property "currentFrame" of "fast" is 2

  Scenario: A paused animation stays on its frame and is still playing
    Given the QML:
      """qml
      import QtQuick
      Item {
        AnimatedImage { objectName: "lights"; source: "lights.gif"; paused: true }
      }
      """
    When 500 milliseconds pass
    Then the property "currentFrame" of "lights" is 0
    And the property "playing" of "lights" is true

  Scenario: The author can turn to a frame
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 30; height: 20
        AnimatedImage {
          id: lights; objectName: "lights"; source: "lights.gif"; paused: true
          Component.onCompleted: currentFrame = 2
        }
      }
      """
    Then the property "currentFrame" of "lights" is 2
    And the pixel at 15, 10 of the window is "#0000ff"

  Scenario: Stopping keeps the frame on screen and lets go of the pause
    Given the QML:
      """qml
      import QtQuick
      Item {
        AnimatedImage {
          id: lights; objectName: "lights"; source: "lights.gif"; paused: true
          Component.onCompleted: { currentFrame = 2; playing = false }
        }
      }
      """
    When 500 milliseconds pass
    Then the property "currentFrame" of "lights" is 2
    And the property "paused" of "lights" is false
    And the property "playing" of "lights" is false

  Scenario: An animation that is not playing stays on its first frame
    Given the QML:
      """qml
      import QtQuick
      Item {
        AnimatedImage { objectName: "lights"; source: "lights.gif"; playing: false }
      }
      """
    When 500 milliseconds pass
    Then the property "currentFrame" of "lights" is 0
    And the property "frameCount" of "lights" is 3

  Scenario: A picture of one frame does not play
    Given the QML:
      """qml
      import QtQuick
      Item {
        AnimatedImage { objectName: "still"; source: "still.gif" }
      }
      """
    When 500 milliseconds pass
    Then the property "frameCount" of "still" is 1
    And the property "playing" of "still" is false
    And the property "status" of "still" is 1

  Scenario: An Image shows the first frame of an animation
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 30; height: 20
        Image { objectName: "picture"; source: "lights.gif" }
      }
      """
    Then the property "status" of "picture" is 1
    And the property "implicitWidth" of "picture" is 30
    When 300 milliseconds pass
    Then the pixel at 15, 10 of the window is "#ff0000"

  # Qt counts the frames of a picture from the network only once it has
  # played them all, so the count is asked for after a round.
  Scenario: An animation from the network loads in the background, then plays
    Given the web server serves a 30 by 20 animated image file "net.gif" of frames "#ff0000, #00ff00, #0000ff" each shown for 200 milliseconds
    And the QML:
      """qml
      import QtQuick
      Item {
        AnimatedImage { objectName: "lights"; source: "$SERVER/net.gif"; asynchronous: false }
      }
      """
    Then the property "status" of "lights" is 2
    When 1 second passes
    Then the property "status" of "lights" is 1
    And the property "frameCount" of "lights" is 3
    And the property "implicitWidth" of "lights" is 30
