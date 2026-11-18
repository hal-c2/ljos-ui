# Sources:
#   examples/car-cockpit/README.md (what the cockpit does)
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle)
#   docs/DIALECT.md (`t` strings, reduced motion)
# The example is written in the Ljós dialect: Qt has nothing to compare it with.

@ljos-only
Feature: The car cockpit example
  `examples/car-cockpit` is an instrument cluster above a touchscreen for
  a made-up car. Everything the car is lives in one object, `car`; the
  pages only read it and call its functions. These scenarios load the
  example as it ships and drive it the way a driver does: by its
  buttons, by name.

  The test window is 400 pixels wide, so each scenario makes the cockpit
  as wide as it, 400 by 225; the cockpit scales itself to its size, as it
  does in any window.

  Background:
    Given the files of the example "car-cockpit"

  Rule: The car's doors, frunk, trunk and charge port open from the car page

    Background:
      When "Main.qml" is loaded
      And the property "width" of "cockpit" is set to 400
      And the property "height" of "cockpit" is set to 225
      And the user clicks the "button" "Car"

    Scenario Outline: Tapping a part opens it, and tapping it again closes it
      When the user clicks the "button" "<button>"
      Then the property "<part>" of "car" is true
      When the user clicks the "button" "<button>"
      Then the property "<part>" of "car" is false

      Examples:
        | button      | part       |
        | Left door   | doorLeft   |
        | Right door  | doorRight  |
        | Frunk       | frunk      |
        | Trunk       | trunk      |
        | Charge port | chargePort |

    Scenario: An opening part swings through the way between shut and open
      When the user clicks the "button" "Trunk"
      And 150 milliseconds pass
      Then the property "trunk" of "carPanel" is at least 0.05
      And the property "trunk" of "carPanel" is at most 0.95
      When 1 second passes
      Then the property "trunk" of "carPanel" is 1

    Scenario: Locking the car closes everything that is open
      When the user clicks the "button" "Left door"
      And the user clicks the "button" "Frunk"
      And the user clicks the "button" "Unlocked"
      Then the property "locked" of "car" is true
      And the property "openCount" of "car" is 0

    Scenario: A locked car keeps its doors shut and says why
      When the user clicks the "button" "Unlocked"
      And the user clicks the "button" "Left door"
      Then the property "doorLeft" of "car" is false
      And the property "refused" of "carPanel" is "The car is locked. Unlock it to open anything."

    Scenario: A moving car keeps its doors shut and says why
      When the property "speed" of "car" is set to 30
      And the user clicks the "button" "Right door"
      Then the property "doorRight" of "car" is false
      And the property "refused" of "carPanel" is "Stop the car first: nothing opens while it moves."

    Scenario: A car out of park keeps its doors shut and says why
      When the user clicks the "button" "N"
      And the user clicks the "button" "Left door"
      Then the property "doorLeft" of "car" is false
      And the property "refused" of "carPanel" is "Shift to P first: nothing opens out of park."

    Scenario: Leaving park closes everything that is open
      When the user clicks the "button" "Trunk"
      And the user clicks the "button" "Frunk"
      And the user clicks the "button" "R"
      Then the property "openCount" of "car" is 0

  Rule: With reduced motion nothing on the car moves, and the change still shows

    Scenario: With reduced motion a door is open at once, and the camera stays where it is
      Given the system setting "reduced motion" is true
      When "Main.qml" is loaded
      And the property "width" of "cockpit" is set to 400
      And the property "height" of "cockpit" is set to 225
      And the user clicks the "button" "Car"
      And the user clicks the "button" "Right door"
      Then the property "doorRight" of "carPanel" is 1
      And the property "yaw" of "carPanel" is 38

  Rule: The car model is a button of its own, named apart from the buttons beside it

    Scenario: The model says which part it is on, and the part's button keeps its name
      When "Main.qml" is loaded
      And the property "width" of "cockpit" is set to 400
      And the property "height" of "cockpit" is set to 225
      And the user clicks the "button" "Car"
      Then the accessibility tree has a "button" named "Car model: Left door"
      And the accessibility tree has a "button" named "Left door"

    Scenario: Pressing the model works the part it is named after
      When "Main.qml" is loaded
      And the property "width" of "cockpit" is set to 400
      And the property "height" of "cockpit" is set to 225
      And the user clicks the "button" "Car"
      And the user clicks the "button" "Trunk"
      Then the accessibility tree has a "button" named "Car model: Trunk"
      When an assistive technology asks the "button" named "Car model: Trunk" to press
      Then the property "trunk" of "car" is false
      And the property "doorLeft" of "car" is false

  Rule: The gear selector sets the gear, and the drive runs only in gear

    Background:
      When "Main.qml" is loaded
      And the property "width" of "cockpit" is set to 400
      And the property "height" of "cockpit" is set to 225

    Scenario Outline: Choosing a gear shows it
      When the user clicks the "button" "<gear>"
      Then the property "gear" of "car" is "<gear>"

      Examples:
        | gear |
        | R    |
        | N    |
        | D    |
        | P    |

    Scenario: Parked, no timer of the drive runs
      Then the property "gear" of "car" is "P"
      And the property "running" of "drive" is false
      And the property "running" of "player" is false

    Scenario: In drive the car picks up speed each second
      When the user clicks the "button" "D"
      Then the property "running" of "drive" is true
      When 3 seconds pass
      Then the property "speed" of "car" is at least 5

    Scenario: With the simulated drive stopped, a car in drive stays where it is
      When the property "simulate" of "car" is set to false
      And the user clicks the "button" "D"
      Then the property "running" of "drive" is false
      When 3 seconds pass
      Then the property "speed" of "car" is 0

  Rule: The player plays, pauses and skips

    Background:
      When "Main.qml" is loaded
      And the property "width" of "cockpit" is set to 400
      And the property "height" of "cockpit" is set to 225
      And the user clicks the "button" "Media"

    Scenario: Play counts the seconds of the track, and pause stops them
      When the user clicks the "button" "Play"
      Then the property "playing" of "car" is true
      When 2 seconds pass
      Then the property "position" of "car" is at least 2
      When the user clicks the "button" "Pause"
      Then the property "playing" of "car" is false
      And the property "running" of "player" is false

    Scenario: The next track starts at its beginning
      When the user clicks the "button" "Next track"
      Then the property "track" of "car" is 1
      And the property "position" of "car" is 0

    Scenario: Before the first track comes the last
      When the user clicks the "button" "Previous track"
      Then the property "track" of "car" is at least 1

  Rule: Settings change the look and the glow

    Background:
      When "Main.qml" is loaded
      And the property "width" of "cockpit" is set to 400
      And the property "height" of "cockpit" is set to 225
      And the user clicks the "button" "Settings"

    Scenario: The cockpit starts dark and turns light
      Then the property "look" of "theme" is "dark"
      When the user clicks the "button" "Light"
      Then the property "dark" of "theme" is false

    Scenario: The glow can be turned off
      Then the property "bloom" of "theme" is true
      When the user clicks the "button" "Off"
      Then the property "bloom" of "theme" is false

  Rule: The clock wakes once a minute, on the minute

    Scenario: The clock's next wake is the start of the next minute
      Given the system clock reads "2026-10-11T17:57:09Z"
      When "Main.qml" is loaded
      And the property "width" of "cockpit" is set to 400
      And the property "height" of "cockpit" is set to 225
      Then the property "turn.interval" of "clock" is 51000
      When 51 seconds pass
      Then the property "turn.interval" of "clock" is 60000
