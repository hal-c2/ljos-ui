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
  ships and wear it the way its owner does.

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
      Then the property "page" of "watch" is "<page>"

      Examples:
        | from     | to       | page          |
        | 227, 400 | 227, 80  | notifications |
        | 227, 60  | 227, 380 | settings      |
        | 400, 227 | 60, 227  | launcher      |

    Scenario: A swipe to the right goes back to the face
      When the user drags from 400, 227 to 60, 227
      And the user drags from 60, 227 to 400, 227
      Then the property "page" of "watch" is "face"

    Scenario: The side button comes home from any screen
      When the user drags from 400, 227 to 60, 227
      And the user clicks the "button" "Timer"
      And the user presses "Escape"
      Then the property "page" of "watch" is "face"

  Rule: The crown moves through a list, and a tap opens what it points at

    Background:
      When "Main.qml" is loaded
      And the user drags from 400, 227 to 60, 227

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

  Rule: Notifications open, and are dismissed

    Background:
      When "Main.qml" is loaded
      And the user drags from 227, 400 to 227, 80

    Scenario: The watch starts with unread notifications
      Then the property "unread" of "watch" is 4

    Scenario: Tapping a notification opens it, and dismissing it takes it away
      When the user clicks the "button" "Open the message from Ásta"
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

    Scenario: The heart screen samples once a second and draws the beats
      When the user drags from 400, 227 to 60, 227
      And the user clicks the "button" "Heart"
      Then the property "running" of "beat" is true
      When 3 seconds pass
      Then the property "bpm" of "watch" is at least 50
      And the property "bpm" of "watch" is at most 180
      And the property "samples" of "watch" is at least 3

  Rule: A workout counts its time, pauses and ends

    Background:
      When "Main.qml" is loaded
      And the user drags from 400, 227 to 60, 227
      And the user clicks the "button" "Workout"

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

  Rule: The timer counts down to its end, and the stopwatch counts up

    Background:
      When "Main.qml" is loaded
      And the user drags from 400, 227 to 60, 227

    Scenario: The crown sets the timer's minutes
      When the user clicks the "button" "Timer"
      Then the property "timerMinutes" of "watch" is 5
      When the user turns the wheel 3 notches down at 227, 227
      Then the property "timerMinutes" of "watch" is 8

    Scenario: A one-minute timer rings when its minute is up
      When the user clicks the "button" "Timer"
      And the user turns the wheel 4 notches up at 227, 227
      And the user clicks the "button" "Start timer"
      Then the property "timerState" of "watch" is "running"
      When 61 seconds pass
      Then the property "timerState" of "watch" is "ringing"

    Scenario: The stopwatch counts, keeps laps, and stops
      When the user clicks the "button" "Stopwatch"
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
      And the user clicks the "button" "Music"

    Scenario: Play and pause
      When the user clicks the "button" "Play"
      Then the property "playing" of "watch" is true
      When the user clicks the "button" "Pause"
      Then the property "playing" of "watch" is false

    Scenario: The next track
      When the user clicks the "button" "Next track"
      Then the property "track" of "watch" is 1

    Scenario: The crown turns the volume up
      Then the property "volume" of "watch" is 6
      When the user turns the wheel 2 notches up at 227, 227
      Then the property "volume" of "watch" is 8

  Rule: Quick settings switch the watch's modes

    Background:
      When "Main.qml" is loaded
      And the user drags from 227, 60 to 227, 380

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

    Scenario: With always on switched off the screen goes dark instead
      When the property "alwaysOn" of "watch" is set to false
      And 16 seconds pass
      Then the property "ambient" of "watch" is true
      And the property "dark" of "display" is true

    Scenario: A running stopwatch keeps its time while the screen sleeps
      When the user drags from 400, 227 to 60, 227
      And the user clicks the "button" "Stopwatch"
      And the user clicks the "button" "Start stopwatch"
      And 20 seconds pass
      Then the property "ambient" of "watch" is true
      And the property "running" of "tick" is false
      When the user presses "Escape"
      Then the property "stopwatchMs" of "watch" is at least 20000

  Rule: With reduced motion screens still move between each other; only less

    Scenario: With reduced motion a screen still slides in
      Given the system setting "reduced motion" is true
      When "Main.qml" is loaded
      And the user drags from 400, 227 to 60, 227
      And 60 milliseconds pass
      Then the property "shift" of "pages" is at least 0.05
      And the property "shift" of "pages" is at most 0.95
      When 1 second passes
      Then the property "shift" of "pages" is 1
