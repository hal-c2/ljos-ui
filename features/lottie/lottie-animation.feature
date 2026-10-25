# Sources:
#   https://doc.qt.io/qt-6/qtlottieanimation-index.html
#   https://doc.qt.io/qt-6/qml-qt-labs-lottieqt-lottieanimation.html
#   https://lottiefiles.github.io/lottie-docs/
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: LottieAnimation plays a Lottie file
  A LottieAnimation from Qt.labs.lottieqt loads a Lottie (Bodymovin) JSON
  file and plays it at the rate the file sets, from its first to its last
  frame, as many times as it is told. It says how its loading went, can be
  paused, stopped, sent to a frame or a marker, and says when it is done.

  The slide is two seconds long at ten frames a second: a white bar 12 wide
  moves from x 10 to x 90, four pixels a frame. The blink is half a second.

  Background:
    Given the text file "slide.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":20,"w":100,"h":100,
       "markers":[{"tm":5,"cm":"middle","dr":0}],
       "layers":[{"ty":4,"ind":1,"ip":0,"op":20,"st":0,
        "ks":{"p":{"a":1,"k":[
          {"t":0,"s":[10,50],"o":{"x":[0],"y":[0]},"i":{"x":[1],"y":[1]}},
          {"t":20,"s":[90,50]}]}},
        "shapes":[{"ty":"rc","p":{"a":0,"k":[0,0]},"s":{"a":0,"k":[12,20]},"r":{"a":0,"k":0}},
          {"ty":"fl","c":{"a":0,"k":[1,1,1,1]},"o":{"a":0,"k":100}}]}]}
      """
    And the text file "blink.json":
      """
      {"v":"5.7.0","fr":20,"ip":0,"op":10,"w":40,"h":30,
       "layers":[{"ty":1,"sc":"#ffffff","sw":40,"sh":30,"ip":0,"op":10,"st":0,"ks":{}}]}
      """

  Rule: Loading

    Scenario: Without a source nothing is loaded and the defaults show
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property string seen: "?"
          LottieAnimation { id: anim }
          Component.onCompleted: seen = [anim.status, anim.frameRate, anim.loops,
            anim.autoPlay, anim.direction, anim.quality, anim.startFrame,
            anim.endFrame, anim.getDuration()].join(" ")
        }
        """
      Then the property "seen" of "root" is "0 30 1 true 1 2 0 0 0"

    Scenario: The names of the states, directions and qualities
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          objectName: "root"
          property string seen: [LottieAnimation.Null, LottieAnimation.Loading,
            LottieAnimation.Ready, LottieAnimation.Error, LottieAnimation.Forward,
            LottieAnimation.Reverse, LottieAnimation.LowQuality,
            LottieAnimation.MediumQuality, LottieAnimation.HighQuality,
            LottieAnimation.Infinite].join(" ")
        }
        """
      Then the property "seen" of "root" is "0 1 2 3 1 -1 0 1 2 -1"

    Scenario: A local file is ready by the time the component is complete
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property string seen: "?"
          LottieAnimation { id: anim; source: "slide.json" }
          Component.onCompleted: seen = [anim.status, anim.startFrame,
            anim.endFrame, anim.frameRate, anim.getDuration(true),
            anim.getDuration(false)].join(" ")
        }
        """
      Then the property "seen" of "root" is "2 0 20 10 20 2"

    Scenario: Loading is announced, then being ready
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property var states: []
          LottieAnimation {
            source: "slide.json"
            onStatusChanged: root.states = root.states.concat([status])
          }
        }
        """
      Then the property "states" of "root" is [1, 2]

    Scenario: The animation takes the size of the file's canvas
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          LottieAnimation { objectName: "anim"; source: "blink.json"; width: 10 }
        }
        """
      Then the property "width" of "anim" is 40
      And the property "height" of "anim" is 30

    Scenario: The file's frame rate replaces the one written in QML
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          LottieAnimation { objectName: "anim"; source: "slide.json"; frameRate: 5 }
        }
        """
      Then the property "frameRate" of "anim" is 10

    Scenario: The first and last frames come from the file and cannot be set
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          LottieAnimation { objectName: "anim"; source: "slide.json" }
        }
        """
      Then setting the property "startFrame" of "anim" to 3 is rejected
      And setting the property "endFrame" of "anim" to 3 is rejected
      And the property "endFrame" of "anim" is 20

    Scenario: A file that is not there is an error
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property var states: []
          LottieAnimation {
            source: "nowhere.json"
            onStatusChanged: root.states = root.states.concat([status])
          }
        }
        """
      Then the property "states" of "root" is [1, 3]

    Scenario: A file that is not JSON is an error, and says why
      Given the text file "broken.json":
        """
        {"v":"5.7.0","fr":10,
        """
      And the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          LottieAnimation { objectName: "anim"; source: "broken.json" }
        }
        """
      Then the property "status" of "anim" is 3
      And the log has a warning matching "JSON parse error"

    Scenario: JSON that is not an animation is an error
      Given the text file "nothing.json":
        """
        {}
        """
      And the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          LottieAnimation { objectName: "anim"; source: "nothing.json" }
        }
        """
      Then the property "status" of "anim" is 3

    Scenario: Taking the source away leaves nothing loaded but keeps the numbers
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          LottieAnimation { objectName: "anim"; source: "slide.json" }
        }
        """
      When the property "source" of "anim" is set to ""
      Then the property "status" of "anim" is 0
      And the property "endFrame" of "anim" is 20
      And the property "frameRate" of "anim" is 10

    Scenario: Another file replaces the first
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          LottieAnimation { objectName: "anim"; source: "slide.json" }
        }
        """
      When the property "source" of "anim" is set to "blink.json"
      Then the property "status" of "anim" is 2
      And the property "endFrame" of "anim" is 10
      And the property "frameRate" of "anim" is 20

  Rule: Playing

    Scenario: A loaded animation plays once and says it has finished
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property int finishes: 0
          LottieAnimation { source: "slide.json"; onFinished: root.finishes++ }
        }
        """
      When 1500 milliseconds pass
      Then the property "finishes" of "root" is 0
      When 1500 milliseconds pass
      Then the property "finishes" of "root" is 1

    Scenario: It plays from the first frame across to the last
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          LottieAnimation { source: "slide.json" }
        }
        """
      Then the pixel at 12, 50 of the window is "#ffffff"
      When 1000 milliseconds pass
      Then the pixel at 12, 50 of the window is "#000000"
      When 1500 milliseconds pass
      Then the pixel at 90, 50 of the window is "#ffffff"

    Scenario: Without autoPlay it waits on its first frame
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          width: 100; height: 100
          property int finishes: 0
          LottieAnimation {
            source: "slide.json"; autoPlay: false; onFinished: root.finishes++
          }
        }
        """
      When 2500 milliseconds pass
      Then the property "finishes" of "root" is 0
      And the pixel at 12, 50 of the window is "#ffffff"

    Scenario: A faster frame rate plays the file in less time
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property int finishes: 0
          LottieAnimation {
            source: "slide.json"; onFinished: root.finishes++
            onStatusChanged: if (status === LottieAnimation.Ready) frameRate = 40
          }
        }
        """
      When 1000 milliseconds pass
      Then the property "finishes" of "root" is 1

    Scenario: Two loops take twice as long
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property int finishes: 0
          LottieAnimation { source: "blink.json"; loops: 2; onFinished: root.finishes++ }
        }
        """
      When 800 milliseconds pass
      Then the property "finishes" of "root" is 0
      When 800 milliseconds pass
      Then the property "finishes" of "root" is 1

    Scenario: An animation looping without end never finishes
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property int finishes: 0
          LottieAnimation {
            source: "blink.json"; loops: LottieAnimation.Infinite
            onFinished: root.finishes++
          }
        }
        """
      When 2 seconds pass
      Then the property "finishes" of "root" is 0

    Scenario: Played in reverse it starts on the last frame
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          LottieAnimation {
            objectName: "anim"; source: "slide.json"
            autoPlay: false; direction: LottieAnimation.Reverse
          }
        }
        """
      When 100 milliseconds pass
      Then the pixel at 12, 50 of the window is "#ffffff"
      When the method "start" of "anim" is called
      And 150 milliseconds pass
      Then the pixel at 86, 50 of the window is "#ffffff"
      And the pixel at 12, 50 of the window is "#000000"
      When 2500 milliseconds pass
      Then the pixel at 12, 50 of the window is "#ffffff"

    Scenario: An animation that cannot be seen still finishes on time
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property int finishes: 0
          LottieAnimation {
            source: "blink.json"; visible: false; onFinished: root.finishes++
          }
        }
        """
      When 1 second passes
      Then the property "finishes" of "root" is 1

    Scenario: A change of the loops is announced once
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property int changes: 0
          LottieAnimation { objectName: "anim"; onLoopsChanged: root.changes++ }
        }
        """
      When the property "loops" of "anim" is set to 3
      And the property "loops" of "anim" is set to 3
      Then the property "changes" of "root" is 1

    Scenario: The quality can be chosen
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          LottieAnimation { objectName: "anim"; quality: LottieAnimation.LowQuality }
        }
        """
      Then the property "quality" of "anim" is 0

  Rule: Steering

    Scenario: Pausing holds the animation, and playing goes on from there
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property int finishes: 0
          LottieAnimation {
            objectName: "anim"; source: "blink.json"; onFinished: root.finishes++
          }
        }
        """
      When the method "pause" of "anim" is called
      And 1 second passes
      Then the property "finishes" of "root" is 0
      When the method "play" of "anim" is called
      And 1 second passes
      Then the property "finishes" of "root" is 1

    Scenario: Toggling the pause twice plays again
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property int finishes: 0
          LottieAnimation {
            objectName: "anim"; source: "blink.json"; onFinished: root.finishes++
          }
        }
        """
      When the method "togglePause" of "anim" is called
      And 1 second passes
      Then the property "finishes" of "root" is 0
      When the method "togglePause" of "anim" is called
      And 1 second passes
      Then the property "finishes" of "root" is 1

    Scenario: Stopping goes back to the first frame without finishing
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          width: 100; height: 100
          property int finishes: 0
          LottieAnimation {
            objectName: "anim"; source: "slide.json"; onFinished: root.finishes++
          }
        }
        """
      When 1 second passes
      And the method "stop" of "anim" is called
      And 2500 milliseconds pass
      Then the property "finishes" of "root" is 0
      And the pixel at 12, 50 of the window is "#ffffff"

    Scenario: Going to a frame and stopping there
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          LottieAnimation {
            objectName: "anim"; source: "slide.json"; autoPlay: false
          }
        }
        """
      When the method "gotoAndStop" of "anim" is called with 10
      And 500 milliseconds pass
      Then the pixel at 50, 50 of the window is "#ffffff"
      And the pixel at 12, 50 of the window is "#000000"

    Scenario: A frame past the end is the last frame
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          LottieAnimation {
            objectName: "anim"; source: "slide.json"; autoPlay: false
          }
        }
        """
      When the method "gotoAndStop" of "anim" is called with 100
      Then the pixel at 90, 50 of the window is "#ffffff"

    Scenario: Going to a frame and playing on from there
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property int finishes: 0
          LottieAnimation {
            objectName: "anim"; source: "slide.json"; autoPlay: false
            onFinished: root.finishes++
          }
        }
        """
      When the method "gotoAndPlay" of "anim" is called with 15
      And 1 second passes
      Then the property "finishes" of "root" is 1

    Scenario: Going to a marker by its name
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          width: 100; height: 100
          property var found: []
          LottieAnimation { id: anim; source: "slide.json"; autoPlay: false }
          function jump() {
            found = [anim.gotoAndStop("nowhere"), anim.gotoAndStop("middle")]
          }
        }
        """
      When the method "jump" of "root" is called
      Then the property "found" of "root" is [false, true]
      And the pixel at 30, 50 of the window is "#ffffff"
      And the pixel at 12, 50 of the window is "#000000"

  # The oracle's event loop cannot be read; Ljós promises that an animation
  # at rest asks for no time at all.
  @ljos-only
  Rule: An animation at rest does no work

    Scenario: A finished animation has nothing scheduled
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          LottieAnimation { source: "blink.json" }
        }
        """
      When 1 second passes
      Then the application has nothing scheduled

    Scenario: A paused animation has nothing scheduled
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          LottieAnimation { objectName: "anim"; source: "slide.json" }
        }
        """
      When 300 milliseconds pass
      And the method "pause" of "anim" is called
      Then the application has nothing scheduled

    Scenario: An animation that waits to be started has nothing scheduled
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          LottieAnimation { source: "slide.json"; autoPlay: false }
        }
        """
      Then the application has nothing scheduled

    Scenario: An endless animation that cannot be seen has nothing scheduled
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          LottieAnimation {
            source: "blink.json"; loops: LottieAnimation.Infinite; visible: false
          }
        }
        """
      When 300 milliseconds pass
      Then the application has nothing scheduled
