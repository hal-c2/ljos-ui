# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-frameanimation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: FrameAnimation
  A FrameAnimation calls `triggered` once a frame while it runs, and says
  which frame it is, how long the last frame took and how long it has
  run. It asks for frames only while it runs and is not paused.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int ticks: 0
        property int frameAtStop: -1
        FrameAnimation { id: idle; objectName: "idle" }
        FrameAnimation { id: frames; objectName: "frames"; onTriggered: root.ticks++ }
        property bool counts: frames.currentFrame > 0 && root.ticks === frames.currentFrame
        property bool timed: frames.elapsedTime > 0.1 && frames.elapsedTime < 1
          && frames.frameTime > 0 && frames.frameTime < 0.1
          && frames.smoothFrameTime > 0 && frames.smoothFrameTime < 0.1
        property bool keptAtStop: frames.currentFrame === root.frameAtStop && frames.elapsedTime > 0.1
        property bool goneOn: frames.currentFrame > root.frameAtStop
        property bool startedAfresh; property bool wasReset
        function resetAndLook() {
          frames.reset()
          wasReset = frames.currentFrame === 0 && frames.elapsedTime === 0 && frames.frameTime > 0
        }
        function stopAndKeep() { frames.stop(); root.frameAtStop = frames.currentFrame }
        function stopAndStart() {
          stopAndKeep(); frames.start()
          startedAfresh = frames.currentFrame === frameAtStop && frameAtStop > 0 && frames.elapsedTime === 0
        }
        function pauseAndKeep() { frames.pause(); root.frameAtStop = frames.currentFrame }
      }
      """

  Scenario: A FrameAnimation does not run until it is started
    Then the property "running" of "idle" is false
    And the property "paused" of "idle" is false
    And the property "currentFrame" of "idle" is 0
    And the property "elapsedTime" of "idle" is 0
    And the property "frameTime" of "idle" is 0
    And the property "smoothFrameTime" of "idle" is 0

  Scenario: A running FrameAnimation is triggered once a frame
    When the method "start" of "frames" is called
    And 300 milliseconds pass
    Then the property "running" of "frames" is true
    And the property "counts" of "root" is true
    And the property "timed" of "root" is true

  Scenario: Paused, it still runs but counts no frames
    When the method "start" of "frames" is called
    And 200 milliseconds pass
    And the method "pauseAndKeep" of "root" is called
    And 200 milliseconds pass
    Then the property "running" of "frames" is true
    And the property "paused" of "frames" is true
    And the property "keptAtStop" of "root" is true

  Scenario: Resumed, it counts frames again
    When the method "start" of "frames" is called
    And 200 milliseconds pass
    And the method "pauseAndKeep" of "root" is called
    And the method "resume" of "frames" is called
    And 200 milliseconds pass
    Then the property "paused" of "frames" is false
    And the property "goneOn" of "root" is true

  Scenario: Stopped, it keeps the frame and the time it got to
    When the method "start" of "frames" is called
    And 200 milliseconds pass
    And the method "stopAndKeep" of "root" is called
    And 200 milliseconds pass
    Then the property "running" of "frames" is false
    And the property "keptAtStop" of "root" is true

  Scenario: Started again, its time begins again but its frame count goes on
    When the method "start" of "frames" is called
    And 200 milliseconds pass
    And the method "stopAndStart" of "root" is called
    Then the property "startedAfresh" of "root" is true

  Scenario: reset() takes the frame and the time back to 0
    When the method "start" of "frames" is called
    And 200 milliseconds pass
    And the method "resetAndLook" of "root" is called
    Then the property "running" of "frames" is true
    And the property "wasReset" of "root" is true

  Scenario: Paused while stopped, it does not run
    When the property "paused" of "frames" is set to true
    Then the property "running" of "frames" is false
    And the property "paused" of "frames" is true

  Scenario: Set running by a binding, it starts with the item
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "bound"
        property bool going: frames.currentFrame > 0
        FrameAnimation { id: frames; running: true }
      }
      """
    When 100 milliseconds pass
    Then the property "going" of "bound" is true
