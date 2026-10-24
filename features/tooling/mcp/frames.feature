# Sources:
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle) and sections 9
#   (the frame pipeline) and 19.4 (`app_frames`)
#   https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html
#   https://doc.qt.io/qt-6/qml-qtquick-timer.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent reads a timeline of the frames
  `app_frames` keeps, while it is asked to record, where the time of each
  frame went: the engine's passes (timers, animations, layout, creating,
  building the scene), the time inside bindings and laying out text
  wherever they ran, drawing and presenting, and what woke the frame:
  input, a timer, an animation or a message, and whose: the clients whose
  requests it answered, by the names they gave in the request's `_meta`
  or when they connected, so that a client can tell its own looking from
  the application's work. Each window's frames are kept, and say which
  window they were. Nothing is timed until a client asks to record, and
  an application at rest records nothing. The frames kept are a ring: the
  oldest give way.

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
        Text { objectName: "label"; text: "clicked " + top.clicks + ", ticked " + top.ticks }
        MouseArea { objectName: "area"; anchors.fill: parent; onClicked: top.clicks += 1 }
        Timer { interval: 100; running: true; onTriggered: top.ticks += 1 }
        Rectangle {
          objectName: "box"; width: 10; height: 10
          NumberAnimation on x { running: top.sliding; to: 100; duration: 200 }
        }
      }
      """
    And the inspector is connected with the capabilities "inspect, input, mutate"

  Scenario: Nothing is recorded until a client asks
    When the agent calls the tool "app_frames"
    Then the answer's "/recording" is false
    And the answer's "/frames" has 0 entries

  Scenario: A frame a click woke says so, and where its time went
    When the agent calls the tool "app_frames" with:
      """json
      {"record": true}
      """
    And the agent calls the tool "app_click" with:
      """json
      {"target": "#area"}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"max": 1}
      """
    Then the answer's "/recording" is true
    And the answer's "/frames" has 1 entry
    And the answer's "/frames/0/woke" is ["input", "message"]
    And the answer's "/frames/0/binding_runs" is at least 1
    And the answer's "/frames/0/text_layouts" is at least 1
    And the answer's "/frames/0/passes/layout_us" is at least 0
    And the answer's "/frames/0/passes/scene_us" is at least 0
    And the answer's "/frames/0/bindings_us" is at least 0
    And the answer's "/frames/0/text_us" is at least 0
    And the answer's "/frames/0/total_us" is at least 0

  Scenario: A frame a timer woke says so
    When the agent calls the tool "app_frames" with:
      """json
      {"record": true}
      """
    And the agent calls the tool "app_advance" with:
      """json
      {"ms": 150}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"max": 1}
      """
    Then the answer's "/frames/0/woke" is ["timer", "message"]

  Scenario: The frames of a running animation say an animation woke them
    When the agent calls the tool "app_frames" with:
      """json
      {"record": true}
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
      {"max": 1}
      """
    Then the answer's "/frames/0/woke" is ["animation", "message"]

  Scenario: Recording stops when the client says so
    When the agent calls the tool "app_frames" with:
      """json
      {"record": true}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"record": false}
      """
    Then the answer's "/recording" is false
    And the answer's text contains "not recording"

  # A client that names itself as the holder, as the observer does, keeps
  # the recording for itself: another letting go does not end it.
  Scenario: A recording two clients hold lasts until both let it go
    When the agent calls the tool "app_frames" with:
      """json
      {"record": true, "holder": "first"}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"record": true, "holder": "second"}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"record": false, "holder": "first"}
      """
    Then the answer's "/recording" is true
    When the agent calls the tool "app_frames" with:
      """json
      {"record": false, "holder": "second"}
      """
    Then the answer's "/recording" is false

  Scenario: A frame says which client's request woke it
    When the agent calls the tool "app_frames" with:
      """json
      {"record": true}
      """
    And the agent, naming itself "observer", calls the tool "app_click" with:
      """json
      {"target": "#area"}
      """
    And the agent calls the tool "app_frames" with:
      """json
      {"max": 1}
      """
    Then the answer's "/frames/0/woke" is ["input", "message"]
    And the answer's "/frames/0/asked_by" is ["observer"]
    And the answer's "/frames/0/thread_messages" is 0
    And the answer's "/frames/0/window" is null
    And the answer's text contains "asked by"

  # As held traces end: a client is known to be there only while it waits
  # with `app_watch`, and one that has not asked again within the time its
  # last watch said it would wait, and at least a second, is gone.
  Scenario: A recording ends once its holder goes away without stopping it
    When the agent calls the tool "app_frames" with:
      """json
      {"record": true, "holder": "observer"}
      """
    And the agent calls the tool "app_watch" with:
      """json
      {"since": {"traces": 0}, "topics": ["traces"], "wait_ms": 50, "holder": "observer"}
      """
    And the agent calls the tool "app_frames"
    Then the answer's "/recording" is true
    When the agent calls the tool "app_watch" with:
      """json
      {"since": {"traces": 0}, "topics": ["traces"], "wait_ms": 1500}
      """
    And the agent calls the tool "app_frames"
    Then the answer's "/recording" is false
