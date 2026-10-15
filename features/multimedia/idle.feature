# Sources:
#   https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html
#   https://doc.qt.io/qt-6/qml-qtmultimedia-soundeffect.html
#   https://doc.qt.io/qt-6/qml-qtmultimedia-videooutput.html
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle)

# The oracle's event loop cannot be read; Ljós promises that media at rest
# asks for no time at all.
@ljos-only
Feature: Media at rest does no work
  A player that is not playing, a sound that is not playing and a film
  that cannot be seen ask for nothing: no timer, no frame, no decoding.

  Rule: A player at rest

    Background:
      Given the media file "clip.mp4"
      And the QML:
        """qml
        import QtQuick
        import QtMultimedia
        Item {
          width: 100; height: 100
          MediaPlayer {
            objectName: "player"
            source: "clip.mp4"
            audioOutput: AudioOutput {}
            videoOutput: output
          }
          VideoOutput { id: output; anchors.fill: parent }
        }
        """
      When 300 milliseconds pass

    Scenario: A loaded player has nothing scheduled
      Then the application has nothing scheduled

    Scenario: A paused player has nothing scheduled
      When the method "play" of "player" is called
      And 300 milliseconds pass
      And the method "pause" of "player" is called
      And 100 milliseconds pass
      Then the application has nothing scheduled

    Scenario: A stopped player has nothing scheduled
      When the method "play" of "player" is called
      And 300 milliseconds pass
      And the method "stop" of "player" is called
      And 100 milliseconds pass
      Then the application has nothing scheduled

    Scenario: A player that reached the end has nothing scheduled
      When the method "play" of "player" is called
      And 1500 milliseconds pass
      Then the application has nothing scheduled

  Rule: A sound at rest

    Background:
      Given the media file "beep.wav"
      And the QML:
        """qml
        import QtQuick
        import QtMultimedia
        SoundEffect {
          objectName: "sound"
          source: "beep.wav"
        }
        """
      When 300 milliseconds pass

    Scenario: A loaded sound has nothing scheduled
      Then the application has nothing scheduled

    Scenario: A sound that has played has nothing scheduled
      When the method "play" of "sound" is called
      And 500 milliseconds pass
      Then the property "playing" of "sound" is false
      And the application has nothing scheduled

    Scenario: A sound that was stopped has nothing scheduled
      When the method "play" of "sound" is called
      And the method "stop" of "sound" is called
      Then the application has nothing scheduled
