# Sources:
#   https://doc.qt.io/qt-6/qml-qtmultimedia-soundeffect.html
#   https://doc.qt.io/qt-6/qsoundeffect.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: SoundEffect
  A short sound, decoded whole when its source is set and played from
  memory, as a click or a chime is. beep.wav is a quarter of a second.

  Rule: Loading

    @api-QtMultimedia.SoundEffect.status
    Scenario: A sound starts loading and is then ready
      Given the media file "beep.wav"
      And the QML:
        """qml
        import QtQuick
        import QtMultimedia
        SoundEffect { objectName: "sound"; source: "beep.wav" }
        """
      Then the property "status" of "sound" is 1
      When 300 milliseconds pass
      Then the property "status" of "sound" is 2

    @api-QtMultimedia.SoundEffect.loadedChanged
    Scenario: Being loaded is told by the status alone
      Given the media file "beep.wav"
      And the QML:
        """qml
        import QtQuick
        import QtMultimedia
        SoundEffect {
          objectName: "sound"
          source: "beep.wav"
          property string trail: ""
          onLoadedChanged: trail += "loaded=" + isLoaded() + " "
          onStatusChanged: if (status > 1) trail += "status" + status + " "
        }
        """
      When 300 milliseconds pass
      Then the property "trail" of "sound" is "status2 "

    # As with MediaPlayer: Ljós's sound starts loading when it is complete,
    # after the QML's handlers are connected; Qt's when the source is set.
    @backlog
    Scenario: Loading the first source is not told
      Given the media file "beep.wav"
      And the QML:
        """qml
        import QtQuick
        import QtMultimedia
        SoundEffect {
          objectName: "sound"
          source: "beep.wav"
          property string trail: ""
          onStatusChanged: trail += "status" + status + " "
        }
        """
      When 300 milliseconds pass
      Then the property "trail" of "sound" is "status2 "

    Scenario: A sound that is not there is an error, and says so
      Given the QML:
        """qml
        import QtQuick
        import QtMultimedia
        SoundEffect { objectName: "sound"; source: "nope.wav" }
        """
      When 300 milliseconds pass
      Then the property "status" of "sound" is 3
      And the log has a warning matching "QSoundEffect: Error decoding source .*nope\.wav"

    Scenario: A sound played before it is loaded plays once it is
      Given the media file "beep.wav"
      And the QML:
        """qml
        import QtQuick
        import QtMultimedia
        SoundEffect {
          objectName: "sound"
          source: "beep.wav"
          Component.onCompleted: play()
        }
        """
      When 100 milliseconds pass
      Then the property "playing" of "sound" is true

    Scenario Outline: A new sound starts with <property> <value>
      Given the QML:
        """qml
        import QtQuick
        import QtMultimedia
        SoundEffect { objectName: "sound" }
        """
      Then the property "<property>" of "sound" is <value>

      Examples:
        | property       | value |
        | status         | 0     |
        | volume         | 1     |
        | muted          | false |
        | loops          | 1     |
        | loopsRemaining | 0     |
        | playing        | false |

    Scenario: Infinite is -2
      Given the QML:
        """qml
        import QtQuick
        import QtMultimedia
        SoundEffect { objectName: "sound"; property int infinite: SoundEffect.Infinite }
        """
      Then the property "infinite" of "sound" is -2

  Rule: Playing

    Background:
      Given the media file "beep.wav"
      And the QML:
        """qml
        import QtQuick
        import QtMultimedia
        SoundEffect {
          objectName: "sound"
          source: "beep.wav"
          property string trail: ""
          onLoopsRemainingChanged: trail += "left" + loopsRemaining + " "
          onPlayingChanged: trail += "playing=" + playing + " "
          onLoopCountChanged: trail += "loops" + loops + " "
        }
        """
      When 300 milliseconds pass

    @api-QtMultimedia.SoundEffect.play @api-QtMultimedia.SoundEffect.playing
    Scenario: play() plays the sound once
      When the method "play" of "sound" is called
      Then the property "playing" of "sound" is true
      And the property "loopsRemaining" of "sound" is 1
      When 500 milliseconds pass
      Then the property "playing" of "sound" is false
      And the property "trail" of "sound" is "left1 playing=true left0 playing=false "

    @api-QtMultimedia.SoundEffect.loops @api-QtMultimedia.SoundEffect.loopsRemaining
    Scenario: A sound with two loops counts them down
      When the property "loops" of "sound" is set to 2
      And the method "play" of "sound" is called
      Then the property "loopsRemaining" of "sound" is 2
      When 380 milliseconds pass
      Then the property "loopsRemaining" of "sound" is 1
      And the property "playing" of "sound" is true
      When 500 milliseconds pass
      Then the property "trail" of "sound" is "loops2 left2 playing=true left1 left0 playing=false "

    Scenario: An infinite sound plays until it is stopped
      When the property "loops" of "sound" is set to -2
      And the method "play" of "sound" is called
      And 1500 milliseconds pass
      Then the property "playing" of "sound" is true
      When the method "stop" of "sound" is called
      Then the property "playing" of "sound" is false

    @api-QtMultimedia.SoundEffect.stop
    Scenario: stop() ends the sound at once
      When the method "play" of "sound" is called
      And 100 milliseconds pass
      And the method "stop" of "sound" is called
      Then the property "playing" of "sound" is false
      And the property "loopsRemaining" of "sound" is 0

    @api-QtMultimedia.SoundEffect.volume @api-QtMultimedia.SoundEffect.muted
    Scenario: A sound can be muted and turned down while it plays
      When the method "play" of "sound" is called
      And the property "muted" of "sound" is set to true
      And the property "volume" of "sound" is set to 0.5
      Then the property "muted" of "sound" is true
      And the property "volume" of "sound" is 0.5
      And the property "playing" of "sound" is true
