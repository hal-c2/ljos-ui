# Sources:
#   https://doc.qt.io/qt-6/qml-qtmultimedia-videosink.html
#   https://doc.qt.io/qt-6/qml-qtmultimedia-videooutput.html#videoSink-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: VideoSink
  A VideoOutput has a VideoSink, the frames it is given go through.
  videoSize is the size of the video, (-1, -1) with none: a source
  that has a picture gives it while it loads, and stopping or the end
  of the stream takes it away again. videoFrameChanged fires for every
  frame, and once more when the picture is taken away; QML sees no
  value in its argument. A VideoSink of its own is a player's
  videoOutput too. clip.mp4 is 64 by 48, one second long.

  Background:
    Given the media file "clip.mp4"
    And the file "Sinks.qml":
      """qml
      import QtQuick
      import QtMultimedia
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property alias player: player
        property alias output: output
        property int frames: 0
        property int sizes: 0
        property var shown: "none"
        property string out: ""
        MediaPlayer { id: player; objectName: "player"; videoOutput: output }
        VideoOutput { id: output; width: 100; height: 100 }
        Connections {
          target: output.videoSink
          function onVideoFrameChanged(frame) { root.frames++; root.shown = frame }
          function onVideoSizeChanged() { root.sizes++ }
        }
        function size(sink) { return sink.videoSize.width + "x" + sink.videoSize.height }
        function report() { out = [size(output.videoSink), sizes, frames > 0, typeof shown].join(" ") }
      }
      """

  @api-QtMultimedia.VideoOutput.videoSink @api-QtMultimedia.VideoSink.videoSize
  @api-QtMultimedia.VideoSink.subtitleText
  Scenario: An output's sink starts with no video
    Given the QML:
      """qml
      import QtQuick
      Sinks {
        function describe() {
          out = [typeof output.videoSink, output.videoSink === output.videoSink, size(output.videoSink),
                 JSON.stringify(output.videoSink.subtitleText)].join(" ")
        }
      }
      """
    When the method "describe" of "root" is called
    Then the property "out" of "root" is "object true -1x-1 \"\""

  Scenario: A source with a picture gives its size while it loads
    Given the QML:
      """qml
      import QtQuick
      Sinks {
        property string seen: ""
        Connections {
          target: output.videoSink
          function onVideoSizeChanged() { seen = size(output.videoSink) + " " + player.mediaStatus }
        }
        function load() { player.source = "clip.mp4" }
      }
      """
    When the method "load" of "root" is called
    And 300 milliseconds pass
    And the method "report" of "root" is called
    Then the property "out" of "root" is "64x48 1 false string"
    And the property "seen" of "root" is "64x48 1"

  @api-QtMultimedia.VideoSink.videoFrameChanged
  Scenario: Every frame played goes through the sink
    Given the QML:
      """qml
      import QtQuick
      Sinks { function load() { player.source = "clip.mp4" } }
      """
    When the method "load" of "root" is called
    And 300 milliseconds pass
    And the method "play" of "player" is called
    And 300 milliseconds pass
    And the method "report" of "root" is called
    Then the property "out" of "root" is "64x48 1 true undefined"
    And the property "frames" of "root" is at least 2

  Scenario: Stopping takes the video away, and tells the sink
    Given the QML:
      """qml
      import QtQuick
      Sinks {
        property int before: 0
        function load() { player.source = "clip.mp4" }
        function halt() { before = frames; player.stop() }
        function count() { out = [size(output.videoSink), sizes, frames - before].join(" ") }
      }
      """
    When the method "load" of "root" is called
    And 300 milliseconds pass
    And the method "play" of "player" is called
    And 300 milliseconds pass
    And the method "halt" of "root" is called
    And 100 milliseconds pass
    And the method "count" of "root" is called
    Then the property "out" of "root" is "-1x-1 2 1"

  Scenario: At the end of the stream the video is taken away
    Given the QML:
      """qml
      import QtQuick
      Sinks { function load() { player.source = "clip.mp4" } }
      """
    When the method "load" of "root" is called
    And 300 milliseconds pass
    And the method "play" of "player" is called
    And 1500 milliseconds pass
    And the method "report" of "root" is called
    Then the property "out" of "root" is "-1x-1 2 true undefined"

  Scenario: A source again gives its size again
    Given the QML:
      """qml
      import QtQuick
      Sinks {
        function load() { player.source = "clip.mp4" }
        function unload() { player.source = "" }
      }
      """
    When the method "load" of "root" is called
    And 300 milliseconds pass
    And the method "play" of "player" is called
    And 200 milliseconds pass
    And the method "stop" of "player" is called
    And the method "unload" of "root" is called
    And 100 milliseconds pass
    And the method "load" of "root" is called
    And 300 milliseconds pass
    And the method "report" of "root" is called
    Then the property "out" of "root" is "64x48 3 true undefined"

  Scenario: A VideoSink of its own shows a player's frames
    Given the QML:
      """qml
      import QtQuick
      import QtMultimedia
      Sinks {
        property int own: 0
        VideoSink { id: sink; objectName: "sink"; onVideoFrameChanged: own++ }
        function load() { player.videoOutput = sink; player.source = "clip.mp4" }
        function describe() { out = [size(sink), size(output.videoSink), own > 0, frames].join(" ") }
      }
      """
    When the method "describe" of "root" is called
    Then the property "out" of "root" is "-1x-1 -1x-1 false 0"
    When the method "load" of "root" is called
    And 300 milliseconds pass
    And the method "play" of "player" is called
    And 300 milliseconds pass
    And the method "describe" of "root" is called
    Then the property "out" of "root" is "64x48 -1x-1 true 0"

  Scenario: A sink's subtitle text is the program's to write, its size is not
    Given the QML:
      """qml
      import QtQuick
      import QtMultimedia
      Sinks {
        property int subtitles: 0
        VideoSink { id: sink; objectName: "sink"; onSubtitleTextChanged: subtitles++ }
        function write() {
          sink.subtitleText = "Hello"
          var refused = false
          try { sink.videoSize = Qt.size(4, 4) } catch (e) { refused = true }
          var fixed = false
          try { output.videoSink = sink } catch (e) { fixed = true }
          out = [sink.subtitleText, subtitles, refused, size(sink), fixed].join(" ")
        }
      }
      """
    When the method "write" of "root" is called
    Then the property "out" of "root" is "Hello 1 true -1x-1 true"
