# Sources:
#   https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html
#   https://doc.qt.io/qt-6/qml-qtmultimedia-audiooutput.html
#   https://doc.qt.io/qt-6/qmediaplayer.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: MediaPlayer
  A player opens a sound or a film, says what it found, and plays it on
  the AudioOutput and VideoOutput it is given. clip.mp4 is one second of
  64 by 48 film at 10 frames a second, with sound.

  Rule: Opening a file

    Background:
      Given the media file "clip.mp4"
      And the QML:
        """qml
        import QtQuick
        import QtMultimedia
        Item {
          property string trail: ""
          property string title: ""
          MediaPlayer {
            objectName: "player"
            source: "clip.mp4"
            audioOutput: AudioOutput {}
            onMetaDataChanged: title = metaData.stringValue(MediaMetaData.Title)
            onDurationChanged: trail += "duration "
            onTracksChanged: trail += "tracks "
            onSeekableChanged: trail += "seekable "
            onHasAudioChanged: trail += "audio "
            onHasVideoChanged: trail += "video "
            onMediaStatusChanged: {
              if (mediaStatus > 1) trail += "status" + mediaStatus + " "
              first += "status" + mediaStatus + " "
            }
          }
          property string first: ""
          objectName: "root"
        }
        """

    @api-QtMultimedia.MediaPlayer.mediaStatus
    Scenario: A player with a source starts loading it
      Then the property "mediaStatus" of "player" is 1
      And the property "playbackState" of "player" is 0
      And the property "duration" of "player" is 0
      And the property "seekable" of "player" is false

    @api-QtMultimedia.MediaPlayer.duration @api-QtMultimedia.MediaPlayer.hasAudio
    @api-QtMultimedia.MediaPlayer.hasVideo @api-QtMultimedia.MediaPlayer.seekable
    Scenario Outline: Once loaded, the player knows <property>
      When 300 milliseconds pass
      Then the property "<property>" of "player" is <value>

      Examples:
        | property           | value |
        | mediaStatus        | 2     |
        | playbackState      | 0     |
        | duration           | 1000  |
        | position           | 0     |
        | hasAudio           | true  |
        | hasVideo           | true  |
        | seekable           | true  |
        | error              | 0     |
        | errorString        | ""    |
        | playing            | false |
        | bufferProgress     | 0     |
        | audioTracks.length | 1     |
        | videoTracks.length | 1     |
        | activeAudioTrack   | 0     |
        | activeVideoTrack   | 0     |

    Scenario: What a file holds is told before it is loaded
      When 300 milliseconds pass
      Then the property "trail" of "root" is "duration tracks seekable audio video status2 "

    # Ljós's player starts loading when it is complete, after the QML's
    # handlers are connected, so they see Loading arrive; Qt's starts when
    # the source is set, before. A native type has no hook there yet.
    @backlog
    Scenario: Loading the first source is not told
      When 300 milliseconds pass
      Then the property "first" of "root" is "status2 "

    @api-QtMultimedia.MediaPlayer.metaData
    Scenario: The metadata has the file's title
      When 300 milliseconds pass
      Then the property "title" of "root" is "Test Clip"

  Rule: Playing

    Background:
      Given the media file "clip.mp4"
      And the QML:
        """qml
        import QtQuick
        import QtMultimedia
        Item {
          objectName: "root"
          property string trail: ""
          MediaPlayer {
            objectName: "player"
            source: "clip.mp4"
            audioOutput: AudioOutput {}
            onPlayingChanged: trail += "playing=" + playing + " "
            onPlaybackStateChanged: trail += "state" + playbackState + " "
            onMediaStatusChanged: if (mediaStatus > 2) trail += "status" + mediaStatus + " "
          }
        }
        """
      When 300 milliseconds pass

    @api-QtMultimedia.MediaPlayer.play @api-QtMultimedia.MediaPlayer.playing
    @api-QtMultimedia.MediaPlayer.playbackState
    Scenario: play() starts the player and fills its buffer
      When the method "play" of "player" is called
      And 100 milliseconds pass
      Then the property "trail" of "root" is "playing=true state1 status4 status5 "
      And the property "playbackState" of "player" is 1
      And the property "playing" of "player" is true

    @api-QtMultimedia.MediaPlayer.position
    Scenario: The position follows the clock while playing
      When the method "play" of "player" is called
      And 500 milliseconds pass
      Then the property "position" of "player" is within 150 of 500

    Scenario: A player that reaches the end stops there
      When the method "play" of "player" is called
      And 1500 milliseconds pass
      Then the property "trail" of "root" is "playing=true state1 status4 status5 playing=false state0 status6 "
      And the property "position" of "player" is 1000
      And the property "mediaStatus" of "player" is 6

    @api-QtMultimedia.MediaPlayer.loops
    Scenario: A player with two loops plays twice
      When the property "loops" of "player" is set to 2
      And the method "play" of "player" is called
      And 1500 milliseconds pass
      Then the property "playing" of "player" is true
      When 1000 milliseconds pass
      Then the property "playing" of "player" is false
      And the property "mediaStatus" of "player" is 6

    @api-QtMultimedia.MediaPlayer.pause
    Scenario: pause() from loaded pauses at the start, buffering
      When the method "pause" of "player" is called
      Then the property "playbackState" of "player" is 2
      And the property "mediaStatus" of "player" is 4
      And the property "position" of "player" is 0

    @api-QtMultimedia.MediaPlayer.setPosition
    Scenario: A paused player moves at once when the position is set
      When the method "pause" of "player" is called
      And the property "position" of "player" is set to 500
      Then the property "position" of "player" is 500
      And the property "mediaStatus" of "player" is 2
      When 300 milliseconds pass
      Then the property "position" of "player" is 500

    Scenario: A paused player goes on from where it was
      When the method "pause" of "player" is called
      And the property "position" of "player" is set to 500
      And the method "play" of "player" is called
      And 300 milliseconds pass
      Then the property "position" of "player" is within 150 of 800
      And the property "playbackState" of "player" is 1

    @api-QtMultimedia.MediaPlayer.stop
    Scenario: stop() goes back to the start
      When the method "play" of "player" is called
      And 400 milliseconds pass
      And the method "stop" of "player" is called
      Then the property "position" of "player" is 0
      And the property "playbackState" of "player" is 0
      And the property "mediaStatus" of "player" is 2
      And the property "playing" of "player" is false

    @api-QtMultimedia.MediaPlayer.playbackRate
    Scenario: At twice the rate the clip is over in half the time
      When the property "playbackRate" of "player" is set to 2
      And the method "play" of "player" is called
      And 750 milliseconds pass
      Then the property "mediaStatus" of "player" is 6

  Rule: A file that is not there

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtMultimedia
        Item {
          objectName: "root"
          property string trail: ""
          MediaPlayer {
            objectName: "player"
            source: "missing.mp4"
            onErrorOccurred: (error, errorString) => trail += "occurred " + error + " " + errorString + ", "
            onErrorChanged: trail += "error " + error + ", "
            onMediaStatusChanged: if (mediaStatus > 1) trail += "status " + mediaStatus + ", "
          }
        }
        """

    @api-QtMultimedia.MediaPlayer.error @api-QtMultimedia.MediaPlayer.errorOccurred
    @api-QtMultimedia.MediaPlayer.errorString
    Scenario: A missing file is a resource error and invalid media
      When 300 milliseconds pass
      Then the property "trail" of "root" is "occurred 1 Could not open file, error 1, status 7, "
      And the property "error" of "player" is 1
      And the property "errorString" of "player" is "Could not open file"
      And the property "duration" of "player" is 0

    Scenario: Invalid media does not play
      When 300 milliseconds pass
      And the method "play" of "player" is called
      Then the property "playbackState" of "player" is 0
      And the property "playing" of "player" is false

  Rule: Changing the source

    Background:
      Given the media file "clip.mp4"
      And the media file "beep.wav"
      And the QML:
        """qml
        import QtQuick
        import QtMultimedia
        MediaPlayer {
          objectName: "player"
          source: "clip.mp4"
          audioOutput: AudioOutput {}
        }
        """
      When 300 milliseconds pass

    @api-QtMultimedia.MediaPlayer.source
    Scenario: A new source loads from the start
      When the property "source" of "player" is set to "beep.wav"
      Then the property "mediaStatus" of "player" is 1
      When 300 milliseconds pass
      Then the property "mediaStatus" of "player" is 2
      And the property "duration" of "player" is 250
      And the property "hasVideo" of "player" is false

    Scenario: A new source stops what was playing
      When the method "play" of "player" is called
      And 200 milliseconds pass
      And the property "source" of "player" is set to "beep.wav"
      Then the property "playbackState" of "player" is 0
      And the property "position" of "player" is 0

    Scenario: An empty source is no media
      When the property "source" of "player" is set to ""
      Then the property "mediaStatus" of "player" is 0
      And the property "duration" of "player" is 0

  Rule: The sound's output

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtMultimedia
        Item {
          MediaPlayer { audioOutput: output }
          AudioOutput { id: output; objectName: "output" }
        }
        """

    @api-QtMultimedia.AudioOutput.volume @api-QtMultimedia.AudioOutput.muted
    Scenario: An output starts at full volume, not muted
      Then the property "volume" of "output" is 1
      And the property "muted" of "output" is false

    Scenario: An output can be muted and turned down
      When the property "muted" of "output" is set to true
      And the property "volume" of "output" is set to 0.25
      Then the property "muted" of "output" is true
      And the property "volume" of "output" is 0.25
