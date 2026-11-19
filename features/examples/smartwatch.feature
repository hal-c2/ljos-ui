# Sources:
#   examples/smartwatch/README.md (what the watch does)
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle)
#   docs/DIALECT.md (`t` strings, reduced motion)
# The example is written in the Ljós dialect: Qt has nothing to compare it with.

@ljos-only
Feature: The smartwatch example
  `examples/smartwatch` is the software of a made-up watch, for a round
  screen and for a rounded-square one. Everything the watch is lives in
  one object, `watch`; the screens only read it and call its functions.
  The crown turns like a mouse wheel, the side button is the Escape key,
  and a finger swipes and taps. These scenarios load the example as it
  ships and wear it the way its owner does, who taps a screen once it
  has come in.

  Background:
    Given the files of the example "smartwatch"
    And the system clock reads "2026-11-18T09:41:07Z"

  Rule: The watch lays itself out for the screen it has

    Scenario: A square screen is round
      When "Main.qml" is loaded
      Then the property "round" of "display" is true

    Scenario: A tall screen is a rounded square
      When "Main.qml" is loaded
      And the property "width" of "display" is set to 410
      And the property "height" of "display" is set to 502
      Then the property "round" of "display" is false

  Rule: The face shows the time and changes with a long press

    Background:
      When "Main.qml" is loaded

    Scenario: The watch starts on its first face, awake
      Then the property "page" of "watch" is "face"
      And the property "face" of "watch" is 0
      And the property "ambient" of "watch" is false

    Scenario: Holding the face moves to the next face, and the last comes round to the first
      When the user presses the pointer at 227, 227
      And 1 second passes
      And the user releases the pointer at 227, 227
      Then the property "face" of "watch" is 1
      When the user presses the pointer at 227, 227
      And 1 second passes
      And the user releases the pointer at 227, 227
      Then the property "face" of "watch" is 0

  Rule: Swipes leave the face, and the side button always comes home

    Background:
      When "Main.qml" is loaded

    Scenario Outline: A swipe from the face opens a screen
      When the user drags from <from> to <to>
      And 400 milliseconds pass
      Then the property "page" of "watch" is "<page>"

      Examples:
        | from     | to       | page          |
        | 227, 400 | 227, 80  | notifications |
        | 227, 60  | 227, 380 | settings      |
        | 400, 227 | 60, 227  | launcher      |

    Scenario: A swipe to the right goes back to the face
      When the user drags from 400, 227 to 60, 227
      And 400 milliseconds pass
      And the user drags from 60, 227 to 400, 227
      And 400 milliseconds pass
      Then the property "page" of "watch" is "face"

    Scenario: The side button comes home from any screen
      When the user drags from 400, 227 to 60, 227
      And 400 milliseconds pass
      And the user clicks the "button" "Timer"
      And 400 milliseconds pass
      And the user presses "Escape"
      Then the property "page" of "watch" is "face"

  Rule: The crown moves through a list, and a tap opens what it points at

    Background:
      When "Main.qml" is loaded
      And the user drags from 400, 227 to 60, 227
      And 400 milliseconds pass

    Scenario: Turning the crown moves the launcher's choice, and pressing it opens the app
      Then the property "current" of "launcher" is 0
      When the user turns the wheel 2 notches down at 227, 227
      Then the property "current" of "launcher" is 2
      When the user turns the wheel 1 notch up at 227, 227
      Then the property "current" of "launcher" is 1
      When the user presses "Return"
      Then the property "page" of "watch" is not "launcher"

    Scenario: The crown stops at the ends of the list
      When the user turns the wheel 1 notch up at 227, 227
      Then the property "current" of "launcher" is 0

    Scenario Outline: Each app of the launcher opens
      When the user clicks the "button" "<app>"
      Then the property "page" of "watch" is "<page>"

      Examples:
        | app           | page          |
        | Heart         | health        |
        | Workout       | workout       |
        | Timer         | timer         |
        | Stopwatch     | stopwatch     |
        | Music         | music         |
        | Notifications | notifications |
        | Settings      | settings      |

    Scenario: Opening an app is a touch, so the watch stays awake for the app
      When 14 seconds pass
      And the user clicks the "button" "Stopwatch"
      And 2 seconds pass
      Then the property "ambient" of "watch" is false
      And the property "page" of "watch" is "stopwatch"

  Rule: Notifications open, and are dismissed

    Background:
      When "Main.qml" is loaded
      And the user drags from 227, 400 to 227, 80
      And 400 milliseconds pass

    Scenario: The watch starts with unread notifications
      Then the property "unread" of "watch" is 4

    Scenario: Tapping a notification opens it, and dismissing it takes it away
      When the user clicks the "button" "Open the message from Ásta"
      And 400 milliseconds pass
      Then the property "page" of "watch" is "notification"
      When the user clicks the "button" "Dismiss"
      Then the property "page" of "watch" is "notifications"
      And the property "unread" of "watch" is 3

    Scenario: Clearing all leaves nothing to read
      When the user clicks the "button" "Clear all"
      Then the property "unread" of "watch" is 0

  Rule: The heart screen samples only while it is shown

    Background:
      When "Main.qml" is loaded

    Scenario: On the face no heart sample is taken
      Then the property "running" of "beat" is false

    Scenario: The heart chart has all its samples from its first frame
      Then the property "history.length" of "watch" is 40
      And the property "historyKeep" of "watch" is 40

    Scenario: A run's heart rate climbs on the heart screen, though nothing else there counts the run's seconds
      When the property "sleeps" of "display" is set to false
      And the method "startWorkout" of "watch" is called
      And the method "show" of "watch" is called with "health"
      And 65 seconds pass
      Then the property "bpm" of "watch" is at least 125

    Scenario: The heart screen samples once a second and draws the beats
      When the user drags from 400, 227 to 60, 227
      And 400 milliseconds pass
      And the user clicks the "button" "Heart"
      And 400 milliseconds pass
      Then the property "running" of "beat" is true
      When 3 seconds pass
      Then the property "bpm" of "watch" is at least 50
      And the property "bpm" of "watch" is at most 180
      And the property "samples" of "watch" is at least 3

  Rule: A workout counts its time, pauses and ends

    Background:
      When "Main.qml" is loaded
      And the user drags from 400, 227 to 60, 227
      And 400 milliseconds pass
      And the user clicks the "button" "Workout"
      And 400 milliseconds pass

    Scenario: A started workout counts its seconds, and a paused one stops counting
      When the user clicks the "button" "Start run"
      Then the property "workout" of "watch" is "running"
      When 5 seconds pass
      Then the property "workoutSeconds" of "watch" is at least 5
      When the user clicks the "button" "Pause"
      And 5 seconds pass
      Then the property "workoutSeconds" of "watch" is at most 6

    Scenario: Ending a workout leaves its summary
      When the user clicks the "button" "Start run"
      And 2 seconds pass
      And the user clicks the "button" "Pause"
      And the user clicks the "button" "End"
      Then the property "workout" of "watch" is "done"

  Rule: A finished run stays in the day's exercise ring

    Background:
      When "Main.qml" is loaded
      And the property "sleeps" of "display" is set to false

    Scenario: Closing a run's summary keeps its minute in the exercise ring
      Then the property "rings.exercise" of "watch" is within 0.005 of 0.7333
      When the method "startWorkout" of "watch" is called
      And 60 seconds pass
      And the method "endWorkout" of "watch" is called
      Then the property "rings.exercise" of "watch" is within 0.005 of 0.7667
      When the method "closeWorkout" of "watch" is called
      Then the property "rings.exercise" of "watch" is within 0.005 of 0.7667

    Scenario: A second run adds to the first
      When the method "startWorkout" of "watch" is called
      And 60 seconds pass
      And the method "endWorkout" of "watch" is called
      And the method "closeWorkout" of "watch" is called
      And the method "startWorkout" of "watch" is called
      And 60 seconds pass
      Then the property "rings.exercise" of "watch" is within 0.005 of 0.8
      When the method "endWorkout" of "watch" is called
      And the method "closeWorkout" of "watch" is called
      Then the property "rings.exercise" of "watch" is within 0.005 of 0.8

  Rule: The timer counts down to its end, and the stopwatch counts up

    Background:
      When "Main.qml" is loaded
      And the user drags from 400, 227 to 60, 227
      And 400 milliseconds pass

    Scenario: The crown sets the timer's minutes
      When the user clicks the "button" "Timer"
      And 400 milliseconds pass
      Then the property "timerMinutes" of "watch" is 5
      When the user turns the wheel 3 notches down at 227, 227
      Then the property "timerMinutes" of "watch" is 8

    Scenario: A one-minute timer rings when its minute is up
      When the user clicks the "button" "Timer"
      And 400 milliseconds pass
      And the user turns the wheel 4 notches up at 227, 227
      And the user clicks the "button" "Start timer"
      Then the property "timerState" of "watch" is "running"
      When 59 seconds pass
      Then the property "timerState" of "watch" is "running"
      When 2 seconds pass
      Then the property "timerState" of "watch" is "ringing"

    Scenario: The countdown says what is left, and that the time is up
      Given an assistive technology is listening
      When the property "sleeps" of "display" is set to false
      And the user clicks the "button" "Timer"
      And 400 milliseconds pass
      Then the accessibility tree has a "label" named "5 minutes"
      When the user clicks the "button" "Start timer"
      And 30 seconds pass
      And 500 milliseconds pass
      Then the accessibility tree has a "label" named "4 minutes 30 seconds left"
      When the user clicks the "button" "Pause timer"
      Then the accessibility tree has a "label" named "4 minutes 29 seconds left, paused"

    Scenario: A countdown that has rung says that the time is up
      Given an assistive technology is listening
      When the user clicks the "button" "Timer"
      And 400 milliseconds pass
      And the user turns the wheel 4 notches up at 227, 227
      And the user clicks the "button" "Start timer"
      And 61 seconds pass
      Then the property "timerState" of "watch" is "ringing"
      And the accessibility tree has a "label" named "Time's up"

    Scenario: A pause that comes after the countdown's end rings instead
      When the user clicks the "button" "Timer"
      And 400 milliseconds pass
      And the user clicks the "button" "Start timer"
      And the property "timerEnds" of "watch" is set to 0
      And the method "pauseTimer" of "watch" is called
      Then the property "timerState" of "watch" is "ringing"

    Scenario: The stopwatch counts, keeps laps, and stops
      When the user clicks the "button" "Stopwatch"
      And 400 milliseconds pass
      And the user clicks the "button" "Start stopwatch"
      And 3 seconds pass
      And the user clicks the "button" "Lap"
      Then the property "laps" of "watch" is 1
      When the user clicks the "button" "Stop stopwatch"
      Then the property "stopwatchMs" of "watch" is at least 3000

  Rule: The player plays and skips, and the crown is its volume

    Background:
      When "Main.qml" is loaded
      And the user drags from 400, 227 to 60, 227
      And 400 milliseconds pass
      And the user clicks the "button" "Music"
      And 400 milliseconds pass

    Scenario: Play and pause
      When the user clicks the "button" "Play"
      Then the property "playing" of "watch" is true
      When the user clicks the "button" "Pause"
      Then the property "playing" of "watch" is false

    Scenario: The next track
      When the user clicks the "button" "Next track"
      Then the property "track" of "watch" is 1

    Scenario: A pause after the track has ended, before anything noticed, is a pause in the next track
      When the property "sleeps" of "display" is set to false
      And the user clicks the "button" "Play"
      And the method "show" of "watch" is called with "settings"
      And 214 seconds pass
      And 500 milliseconds pass
      And the method "pause" of "watch" is called
      Then the property "track" of "watch" is 1
      And the property "position" of "watch" is within 0.1 of 0.5

    Scenario: A pause just before the track ends keeps the track
      When the property "sleeps" of "display" is set to false
      And the user clicks the "button" "Play"
      And the method "show" of "watch" is called with "settings"
      And 213 seconds pass
      And 500 milliseconds pass
      And the method "pause" of "watch" is called
      Then the property "track" of "watch" is 0
      And the property "position" of "watch" is within 0.1 of 213.5

    Scenario: The crown turns the volume up
      Then the property "volume" of "watch" is 6
      When the user turns the wheel 2 notches up at 227, 227
      Then the property "volume" of "watch" is 8

  Rule: Quick settings switch the watch's modes

    Background:
      When "Main.qml" is loaded
      And the user drags from 227, 60 to 227, 380
      And 400 milliseconds pass

    Scenario Outline: A switch turns a mode on and off
      Then the property "<mode>" of "watch" is <before>
      When the user clicks the "button" "<switch>"
      Then the property "<mode>" of "watch" is <after>

      Examples:
        | switch         | mode     | before | after |
        | Do not disturb | quiet    | false  | true  |
        | Always on      | alwaysOn | true   | false |
        | Glow           | bloom    | true   | false |

  Rule: Left alone, the watch dims to its always-on face and wakes only on the minute

    Background:
      When "Main.qml" is loaded

    Scenario: After a while untouched the watch dims, and only the minute clock runs
      When 16 seconds pass
      Then the property "ambient" of "watch" is true
      And the property "running" of "tick" is false
      And the property "running" of "beat" is false
      And the property "running" of "idle" is false
      And the property "turn.running" of "clock" is true

    Scenario: The always-on clock wakes at the start of the next minute
      Then the property "turn.interval" of "clock" is 53000

    Scenario: A tap wakes the watch on its face
      When 16 seconds pass
      And the user clicks the "button" "Wake"
      Then the property "ambient" of "watch" is false
      And the property "page" of "watch" is "face"

    Scenario: Woken, the face shows the second it was woken at, not the one it fell asleep at
      When 36 seconds pass
      And the user clicks the "button" "Wake"
      Then the property "seconds" of "watch" is 43

    Scenario: A swipe on the always-on face wakes the watch as a tap does
      When 16 seconds pass
      And the user drags from 227, 380 to 227, 120
      Then the property "ambient" of "watch" is false

    Scenario: With always on switched off the screen goes dark instead
      When the property "alwaysOn" of "watch" is set to false
      And 16 seconds pass
      Then the property "ambient" of "watch" is true
      And the property "dark" of "display" is true

    Scenario: A running stopwatch keeps its time while the screen sleeps
      When the user drags from 400, 227 to 60, 227
      And 400 milliseconds pass
      And the user clicks the "button" "Stopwatch"
      And 400 milliseconds pass
      And the user clicks the "button" "Start stopwatch"
      And 20 seconds pass
      Then the property "ambient" of "watch" is true
      And the property "running" of "tick" is false
      When the user presses "Escape"
      Then the property "stopwatchMs" of "watch" is at least 20000

  Rule: With reduced motion a screen does not move in; it fades in where it ends

    Scenario: With reduced motion a screen is in place at once and fades in
      Given the system setting "reduced motion" is true
      When "Main.qml" is loaded
      And the user drags from 400, 227 to 60, 227
      And 60 milliseconds pass
      Then the property "shift" of "pages" is 1
      And the property "opacity" of "stage" is at least 0.25
      And the property "opacity" of "stage" is at most 0.95
      When 1 second passes
      Then the property "opacity" of "stage" is 1

    Scenario: Without reduced motion a screen slides in
      When "Main.qml" is loaded
      And the user drags from 400, 227 to 60, 227
      And 60 milliseconds pass
      Then the property "shift" of "pages" is at least 0.05
      And the property "shift" of "pages" is at most 0.95
      When 1 second passes
      Then the property "shift" of "pages" is 1

  Rule: With reduced motion the 3D watch turns five times slower, even when it is asked for while the watch turns

    Scenario: The turntable turns once in 24 seconds
      When "Showcase.qml" is loaded
      And 4 seconds pass
      Then the property "spin" of "showcase" is at least 50
      And the property "spin" of "showcase" is at most 70

    Scenario: Asked for while the watch turns, reduced motion slows the turntable from there
      When "Showcase.qml" is loaded
      And the system setting "reduced motion" changes to true
      And 4 seconds pass
      Then the property "spin" of "showcase" is at least 8
      And the property "spin" of "showcase" is at most 16

    Scenario: Asked for while the viewer drags the watch, reduced motion leaves the turntable waiting
      When "Showcase.qml" is loaded
      And the user presses the pointer at 450, 450
      And the user moves the pointer to 470, 450
      And the user moves the pointer to 520, 450
      And the system setting "reduced motion" changes to true
      And 4 seconds pass
      Then the property "spin" of "showcase" is at most 1
      When the user releases the pointer at 520, 450
      And 4 seconds pass
      Then the property "spin" of "showcase" is at least 8
      And the property "spin" of "showcase" is at most 16

  Rule: On the 3D model nobody can touch the watch, so it stays awake

    Scenario: Left alone, the 3D watch keeps its lit face
      When "Showcase.qml" is loaded
      And 16 seconds pass
      Then the property "ambient" of "watch" is false
