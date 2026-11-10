# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#setSource-method
#   https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html#source-prop
#   https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html#errorOccurred-signal
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A player whose source is a loaded item's first property
  A player's source bound to a property that setSource gives the item is
  set once, when the item is made, and the player opens it once. Writing
  the same source again tells no change and opens nothing. A source that
  cannot be opened is an error that stays.

  Background:
    Given the media file "clip.mp4"
    And the file "Viewer.qml":
      """qml
      import QtQuick
      import QtMultimedia
      Item {
        id: viewer
        objectName: "viewer"
        property url source
        property int changes: 0
        property int errors: 0
        MediaPlayer {
          id: player
          objectName: "player"
          source: viewer.source
          onSourceChanged: viewer.changes++
          onErrorOccurred: viewer.errors++
          onMediaStatusChanged: if (mediaStatus === MediaPlayer.LoadingMedia) viewer.parent.opened++
        }
      }
      """

  Scenario Outline: The player opens <file> once
    Given the QML:
      """qml
      import QtQuick
      Loader {
        objectName: "loader"
        property int opened: 0
        Component.onCompleted: setSource("Viewer.qml", { source: "<file>" })
      }
      """
    When 300 milliseconds pass
    Then the property "changes" of "loader/viewer" is 1
    And the property "opened" of "loader" is 1
    And the property "errors" of "loader/viewer" is <errors>
    And the property "mediaStatus" of "loader/viewer/player" is <status>

    Examples:
      | file        | errors | status |
      | clip.mp4    | 0      | 2      |
      | missing.mp4 | 1      | 7      |

  Scenario Outline: Writing the source it has to <what> changes nothing
    Given the QML:
      """qml
      import QtQuick
      Loader {
        objectName: "loader"
        property int opened: 0
        Component.onCompleted: setSource("Viewer.qml", { source: "clip.mp4" })
      }
      """
    When 300 milliseconds pass
    And the property "source" of "<what>" is set to "clip.mp4"
    And 300 milliseconds pass
    Then the property "changes" of "loader/viewer" is 1
    And the property "opened" of "loader" is 1
    And the property "mediaStatus" of "loader/viewer/player" is 2

    Examples:
      | what                 |
      | loader/viewer        |
      | loader/viewer/player |
