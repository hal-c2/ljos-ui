# Sources:
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle) and section 19.4
#   (`app_frames`' `wakes`)
#   https://doc.qt.io/qt-6/qml-qtquick-timer.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent reads why the application woke
  An application at rest runs nothing: its UI thread sleeps until input,
  a timer, an animation, a message or the platform wakes it. To find what
  keeps one from resting, `app_frames` with `wakes` keeps a ring of the
  UI thread's wakes while a client asks for it: each with its causes
  (input, a timer, an animation, a message and of which kind, a redraw
  the platform asked for, a window that wanted something else) and what
  it led to (the passes run and the frames drawn, or nothing). Nothing is
  kept until a client asks, and while nobody does the loop pays one
  check. A wake only an agent's requests caused does not move the watch's
  `wakes` count, so that a client that follows the wakes does not follow
  its own looking.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: top
        objectName: "root"
        width: 200; height: 100
        property int clicks: 0
        property int ticks: 0
        property bool sliding: false
        Text { text: "clicked " + top.clicks + ", ticked " + top.ticks }
        MouseArea { objectName: "area"; anchors.fill: parent; onClicked: top.clicks += 1 }
        Timer { interval: 100; running: true; onTriggered: top.ticks += 1 }
        Rectangle {
          objectName: "box"; width: 10; height: 10
          NumberAnimation on x { running: top.sliding; to: 100; duration: 200 }
        }
      }
      """
    And the inspector is connected with the capabilities "inspect, input, mutate"

  Scenario: Nothing is kept until a client asks
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true}
      """
    Then the answer's "/recording" is false
    And the answer's "/wakes" has 0 entries

  Scenario: A wake an agent's click caused says so, whose, and what it led to
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "record": true}
      """
    And the agent, naming itself "observer", calls the tool "app_click" with:
      """json
      {"target": "#area"}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "max": 1}
      """
    Then the answer's "/recording" is true
    And the answer's "/wakes" has 1 entry
    And the answer's "/wakes/0/causes" is ["input", "message"]
    And the answer's "/wakes/0/messages/inspector" is 1
    And the answer's "/wakes/0/asked_by" is ["observer"]
    And the answer's "/wakes/0/frames" is at least 1
    And the answer's "/wakes/0/led_to" is "frames"
    And the answer's text contains "led to frames"

  Scenario: A wake a timer caused says so
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "record": true}
      """
    And the agent calls the tool "app_advance" with:
      """json
      {"ms": 150}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "max": 1}
      """
    Then the answer's "/wakes/0/causes" is ["timer", "message"]

  Scenario: A wake a running animation caused says so
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "record": true}
      """
    And the agent calls the tool "app_set_property" with:
      """json
      {"target": "#root", "property": "sliding", "value": true}
      """
    And the agent calls the tool "app_advance" with:
      """json
      {"ms": 50}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "max": 1}
      """
    Then the answer's "/wakes/0/causes" is ["animation", "message"]

  # Reading the wakes is a wake too: the one before it is the read that
  # started the recording, which changed nothing in the application. The
  # test host runs a pass at each turn, so the frame is run and draws
  # nothing; a desktop host runs no pass then, and the wake led to
  # "nothing".
  Scenario: A wake that drew nothing says what it led to
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "record": true}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "max": 1}
      """
    Then the answer's "/wakes/0/causes" is ["message"]
    And the answer's "/wakes/0/drawn" is 0
    And the answer's "/wakes/0/led_to" is "frames"

  Scenario: Frames are recorded apart from wakes
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "record": true}
      """
    And the agent calls the tool "app_frames"
    Then the answer's "/recording" is false

  Scenario: A wake only an agent's requests caused moves no count
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "record": true, "holder": "observer"}
      """
    And the agent calls the tool "app_watch" with:
      """json
      {"since": {"wakes": 0}, "topics": ["wakes"], "wait_ms": 50, "holder": "observer"}
      """
    Then the answer's "/waited" is true
    And the answer's "/counts/wakes" is 0

  Scenario: A wake the application had moves the count
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "record": true, "holder": "observer"}
      """
    And the agent calls the tool "app_click" with:
      """json
      {"target": "#area"}
      """
    And the agent calls the tool "app_watch" with:
      """json
      {"since": {"wakes": 0}, "topics": ["wakes"], "wait_ms": 50, "holder": "observer"}
      """
    Then the answer's "/waited" is false
    And the answer's "/changed/0" is "wakes"

  Scenario: The wakes are kept until the holder goes away
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "record": true, "holder": "observer"}
      """
    And the agent calls the tool "app_watch" with:
      """json
      {"since": {"traces": 0}, "topics": ["traces"], "wait_ms": 50, "holder": "observer"}
      """
    And the agent calls the tool "app_watch" with:
      """json
      {"since": {"traces": 0}, "topics": ["traces"], "wait_ms": 1500}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"wakes": true}
      """
    Then the answer's "/recording" is false
