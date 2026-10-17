# Sources:
#   https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html#source-prop
#   https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html#mediaStatus-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Video from a web server
  A MediaPlayer's source may be on a web server. A stream of pictures
  with no container, MJPEG or H.264, has no length to tell: the player
  is loaded with the size of its first picture and a duration of 0,
  plays every picture there is and reaches the end. clip.mjpeg and
  clip.h264 are ten pictures of 64 by 48.

  Background:
    Given the web server serves the media file "clip.mjpeg"
    And the web server serves the media file "clip.h264"
    And the file "Watch.qml":
      """qml
      import QtQuick
      import QtMultimedia
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property alias player: player
        property int frames: 0
        property string loaded: ""
        MediaPlayer {
          id: player; objectName: "player"; videoOutput: output
          onMediaStatusChanged: if (mediaStatus === MediaPlayer.LoadedMedia)
            root.loaded = [duration, hasVideo, output.videoSink.videoSize.width + "x" + output.videoSink.videoSize.height].join(" ")
        }
        VideoOutput { id: output; anchors.fill: parent }
        Connections {
          target: output.videoSink
          function onVideoFrameChanged() { root.frames++ }
        }
      }
      """

  Scenario Outline: A stream of <kind> pictures loads with its first picture's size
    Given the QML:
      """qml
      import QtQuick
      Watch { player.source: "$SERVER/<file>" }
      """
    Then the property "mediaStatus" of "player" is 1
    When 1 second passes
    Then the property "mediaStatus" of "player" is 2
    And the property "loaded" of "root" is "0 true 64x48"
    And the property "frames" of "root" is 0

    Examples:
      | kind  | file       |
      | JPEG  | clip.mjpeg |
      | H.264 | clip.h264  |

  Scenario Outline: A stream of <kind> pictures plays every picture to the end
    Given the QML:
      """qml
      import QtQuick
      Watch { player.source: "$SERVER/<file>"; Component.onCompleted: player.play() }
      """
    When 2 seconds pass
    Then the property "mediaStatus" of "player" is 6
    And the property "playbackState" of "player" is 0
    And the property "frames" of "root" is 11
    And the property "duration" of "player" is 0
    And the log has no warnings

    Examples:
      | kind  | file       |
      | JPEG  | clip.mjpeg |
      | H.264 | clip.h264  |
