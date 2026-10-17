# Sources:
#   https://doc.qt.io/qt-6/qml-qtmultimedia-videooutput.html
#   https://doc.qt.io/qt-6/qml-qtmultimedia-video.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: VideoOutput and Video
  A VideoOutput shows the frames of the player it is given, fitted into
  itself as its fill mode says. sourceRect is the frame's own size and
  contentRect where it is drawn. A Video is a VideoOutput with its own
  player. clip.mp4 is 64 by 48, one second long.

  Rule: Fitting a frame

    Background:
      Given the media file "clip.mp4"
      And the QML:
        """qml
        import QtQuick
        import QtMultimedia
        Item {
          width: 200; height: 200
          MediaPlayer {
            objectName: "player"
            source: "clip.mp4"
            videoOutput: output
          }
          VideoOutput { id: output; objectName: "output"; width: 100; height: 100 }
        }
        """
      When 300 milliseconds pass

    @api-QtMultimedia.VideoOutput.fillMode @api-QtMultimedia.VideoOutput.sourceRect
    @api-QtMultimedia.VideoOutput.contentRect
    Scenario Outline: Before a frame is shown, the output's <property> is <value>
      Then the property "<property>" of "output" is <value>

      Examples:
        | property           | value |
        | fillMode           | 1     |
        | orientation        | 0     |
        | endOfStreamPolicy  | 0     |
        | sourceRect.width   | 0     |
        | sourceRect.height  | 0     |
        | contentRect.width  | 100   |
        | contentRect.height | 100   |

    Scenario Outline: A playing frame keeps its shape: <property> is <value>
      When the method "play" of "player" is called
      And 200 milliseconds pass
      Then the property "<property>" of "output" is <value>

      Examples:
        | property           | value |
        | sourceRect.width   | 64    |
        | sourceRect.height  | 48    |
        | contentRect.x      | 0     |
        | contentRect.y      | 12.5  |
        | contentRect.width  | 100   |
        | contentRect.height | 75    |

    Scenario: Stretched, a frame fills the output
      When the property "fillMode" of "output" is set to 0
      And the method "play" of "player" is called
      And 200 milliseconds pass
      Then the property "contentRect.y" of "output" is 0
      And the property "contentRect.height" of "output" is 100

    Scenario: Cropped, a frame covers the output and spills over
      When the property "height" of "output" is set to 50
      And the property "fillMode" of "output" is set to 2
      And the method "play" of "player" is called
      And 200 milliseconds pass
      Then the property "contentRect.y" of "output" is -12.5
      And the property "contentRect.width" of "output" is 100
      And the property "contentRect.height" of "output" is 75

    Scenario: The content follows the output's size
      When the method "play" of "player" is called
      And 200 milliseconds pass
      And the property "width" of "output" is set to 200
      Then the property "contentRect.x" of "output" is within 0.01 of 33.33
      And the property "contentRect.width" of "output" is within 0.01 of 133.33

    @api-QtMultimedia.VideoOutput.orientation
    Scenario: A quarter turn turns the content, and a turn that is no quarter is not taken
      When the method "play" of "player" is called
      And 200 milliseconds pass
      And the property "orientation" of "output" is set to 90
      And 100 milliseconds pass
      Then the property "contentRect.x" of "output" is 12.5
      And the property "contentRect.width" of "output" is 75
      And the property "contentRect.height" of "output" is 100
      And the property "sourceRect.width" of "output" is 64
      When the property "orientation" of "output" is set to 45
      And 100 milliseconds pass
      Then the property "contentRect.x" of "output" is 12.5
      And the property "contentRect.width" of "output" is 75
      When the property "orientation" of "output" is set to -180
      And 100 milliseconds pass
      Then the property "contentRect.y" of "output" is 12.5
      And the property "contentRect.width" of "output" is 100

    @api-QtMultimedia.VideoOutput.endOfStreamPolicy
    Scenario: At the end the output is cleared
      When the method "play" of "player" is called
      And 1500 milliseconds pass
      Then the property "sourceRect.width" of "output" is 0
      And the property "contentRect.height" of "output" is 100

  Rule: Video

    Background:
      Given the media file "clip.mp4"
      And the QML:
        """qml
        import QtQuick
        import QtMultimedia
        Video {
          objectName: "video"
          width: 100; height: 100
          source: Qt.resolvedUrl("clip.mp4")
          property string trail: ""
          onPlaying: trail += "playing "
          onPaused: trail += "paused "
          onStopped: trail += "stopped "
        }
        """
      When 300 milliseconds pass

    @api-QtMultimedia.Video.duration @api-QtMultimedia.Video.hasVideo
    Scenario: A Video knows what it was given
      Then the property "duration" of "video" is 1000
      And the property "hasVideo" of "video" is true
      And the property "hasAudio" of "video" is true
      And the property "seekable" of "video" is true
      And the property "playbackState" of "video" is 0

    @api-QtMultimedia.Video.play @api-QtMultimedia.Video.pause @api-QtMultimedia.Video.stop
    Scenario: A Video tells when it plays, pauses and stops
      When the method "play" of "video" is called
      And 200 milliseconds pass
      And the method "pause" of "video" is called
      And the method "stop" of "video" is called
      Then the property "trail" of "video" is "playing paused stopped "

    @api-QtMultimedia.Video.fillMode
    Scenario: A Video keeps the frame's shape unless told otherwise
      Then the property "fillMode" of "video" is 1

    @api-QtMultimedia.Video.seek
    Scenario: seek() moves a paused Video
      When the method "pause" of "video" is called
      And the method "seek" of "video" is called with 500
      Then the property "position" of "video" is 500

    Scenario: A Video plays to its end
      When the method "play" of "video" is called
      And 1500 milliseconds pass
      Then the property "playbackState" of "video" is 0
      And the property "position" of "video" is 1000
      And the property "trail" of "video" is "playing stopped "
