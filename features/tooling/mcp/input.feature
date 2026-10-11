# Sources:
#   docs/ARCHITECTURE.md sections 19.4, 19.6 and 19.7
#   docs/research/10-mcp.md
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html
#   https://doc.qt.io/qt-6/qml-qtquick-keys.html
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html
#   https://doc.qt.io/qt-6/qml-qtquick-timer.html
#   https://doc.qt.io/qt-6/qstylehints.html#colorScheme-prop
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent that may give input
  With the capability `input` an agent can click, type, press keys, scroll,
  touch, move the clock, resize the window and change what the system
  prefers. Each of these is an event like a person's: it goes through the
  same hit test, focus and text input as the real one, so an item that is
  covered is not clicked and a field without focus gets no text. Each
  answers once the frame has settled, and says what changed.

  Background:
    Given the file "Btn.qml":
      """qml
      import QtQuick
      Rectangle {
        id: btn
        property string label
        property string shown: label
        signal hit
        signal twice
        width: 80; height: 30
        Accessible.role: Accessible.Button
        Accessible.name: shown
        Text { anchors.centerIn: parent; text: btn.shown }
        MouseArea { anchors.fill: parent; onClicked: btn.hit(); onDoubleClicked: btn.twice() }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Rectangle {
        id: top
        objectName: "root"
        width: 300; height: 200
        property int clicks: 0
        property int covered: 0
        property int taps: 0
        property int doubles: 0
        property int ticks: 0
        property int enters: 0
        property var scheme: Qt.styleHints.colorScheme
        function drop() { children[0].destroy() }
        Btn { objectName: "save"; label: "Save"; x: 10; y: 10; shown: top.clicks > 0 ? "Saved" : "Save"; onHit: top.clicks += 1; onTwice: top.doubles += 1 }
        Btn { objectName: "locked"; label: "Locked"; x: 10; y: 50; enabled: false; onHit: top.clicks += 100 }
        Btn { objectName: "far"; label: "Far"; x: 900; y: 10; onHit: top.clicks += 100 }
        Btn { objectName: "under"; label: "Under"; x: 10; y: 90; onHit: top.clicks += 100 }
        Rectangle { objectName: "cover"; x: 10; y: 90; width: 80; height: 30; color: "#80ff0000"; MouseArea { anchors.fill: parent; onClicked: top.covered += 1 } }
        TextInput { activeFocusOnTab: true; objectName: "first"; x: 100; y: 10; width: 120; height: 20; focus: true; Accessible.name: "First"; Keys.onReturnPressed: top.enters += 1 }
        TextInput { activeFocusOnTab: true; objectName: "second"; x: 100; y: 40; width: 120; height: 20; Accessible.name: "Second" }
        TextInput { activeFocusOnTab: true; objectName: "secret"; x: 100; y: 70; width: 120; height: 20; echoMode: TextInput.Password; Accessible.name: "Secret" }
        Flickable { objectName: "flick"; x: 100; y: 100; width: 100; height: 60; contentWidth: 100; contentHeight: 600; Rectangle { width: 100; height: 600; color: "#cccccc" } }
        Rectangle { objectName: "tapper"; x: 220; y: 100; width: 60; height: 40; Accessible.role: Accessible.Button; Accessible.name: "Tapper"; TapHandler { onTapped: top.taps += 1 } }
        Timer { running: true; repeat: true; interval: 100; onTriggered: top.ticks += 1 }
      }
      """
    And the inspector is connected with the capabilities "inspect, input"
    And the agent has requested a snapshot

  Rule: A click is a press and a release where the item is

    Scenario: A click on a reference presses the item
      When the agent calls the tool "app_click" on the reference of the "button" named "Save"
      Then the property "clicks" of "root" is 1

    Scenario: A click on a locator presses the item
      When the agent calls the tool "app_click" with:
        """json
        {"target": "role=button[name=\"Save\"]"}
        """
      Then the property "clicks" of "root" is 1

    Scenario: A click on an object the outline does not list is made where it is
      When the agent calls the tool "app_click" with:
        """json
        {"target": "#cover"}
        """
      Then the property "covered" of "root" is 1

    Scenario: A click lands on what is on top, as a person's would
      When the agent calls the tool "app_click" on the reference of the "button" named "Under"
      Then the property "covered" of "root" is 1
      And the property "clicks" of "root" is 0

    # In Qt the second press of a double click raises doubleClicked and not
    # clicked, so two clicks make one of each.
    Scenario: A double click is a click and then a double click
      When the agent calls the tool "app_click" on the reference of the "button" named "Save" with:
        """json
        {"count": 2}
        """
      Then the property "clicks" of "root" is 1
      And the property "doubles" of "root" is 1

    Scenario: A click says what it changed
      When the agent calls the tool "app_click" on the reference of the "button" named "Save"
      Then the answer's "/epoch" is at least 2
      And the answer's "/changed" is at least 1
      And the answer's "/delta" matches "Saved"
      And the answer's "/settled" is true

    Scenario: A click on an item that is disabled is refused
      When the agent calls the tool "app_click" on the reference of the "button" named "Locked"
      Then the agent is refused with "not_actionable"
      And the refusal mentions "disabled"
      And the property "clicks" of "root" is 0

    Scenario: A click on an item outside the window is refused
      When the agent calls the tool "app_click" on the reference of the "button" named "Far"
      Then the agent is refused with "not_actionable"
      And the refusal mentions "outside the window"
      And the property "clicks" of "root" is 0

    Scenario: A click on an item that is gone is told as a stale reference
      When the method "drop" of "root" is called
      And the agent calls the tool "app_click" on the reference of the "button" named "Save"
      Then the agent is told the reference is stale
      And the refusal mentions "app_snapshot"

    Scenario: A click on nothing says so
      When the agent calls the tool "app_click" with:
        """json
        {"target": "#nowhere"}
        """
      Then the agent is refused with "not_found"

    Scenario: A button that is not the left one is a different click
      When the agent calls the tool "app_click" on the reference of the "button" named "Save" with:
        """json
        {"button": "right"}
        """
      Then the property "clicks" of "root" is 0

    Scenario: A button nobody has is refused with its name
      When the agent calls the tool "app_click" on the reference of the "button" named "Save" with:
        """json
        {"button": "fourth"}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "button"

  Rule: Typing goes to the item that has the keyboard

    Scenario: Text is typed into the item with focus
      When the agent calls the tool "app_type" with:
        """json
        {"text": "hello"}
        """
      Then the property "text" of "first" is "hello"
      And the answer's "/typed" is "hello"

    Scenario: Text typed with a target clicks the target first
      When the agent calls the tool "app_type" with:
        """json
        {"text": "second", "target": "#second"}
        """
      Then the property "text" of "second" is "second"
      And the property "text" of "first" is ""
      And the accessibility tree's focus is the "textinput" named "Second"

    Scenario: A character outside ASCII is typed as text
      When the agent calls the tool "app_type" with:
        """json
        {"text": "Þórður"}
        """
      Then the property "text" of "first" is "Þórður"

    Scenario: Typing with the keyboard nowhere is refused
      When the property "focus" of "first" is set to false
      And the agent calls the tool "app_type" with:
        """json
        {"text": "lost"}
        """
      Then the agent is refused with "not_actionable"
      And the refusal mentions "keyboard focus"

    Scenario: What is typed into a field that hides it is not echoed
      When the agent calls the tool "app_type" with:
        """json
        {"text": "hunter2", "target": "#secret"}
        """
      Then the property "text" of "secret" is "hunter2"
      And the answer's "/typed" is "<redacted>"
      And the answer does not contain "hunter2"

  Rule: A key is pressed as a person presses it

    Scenario: Enter reaches the handler of the item with focus
      When the agent calls the tool "app_press" with:
        """json
        {"keys": "Enter"}
        """
      Then the property "enters" of "root" is 1

    Scenario: Tab moves the keyboard to the next item
      When the agent calls the tool "app_press" with:
        """json
        {"keys": "Tab"}
        """
      Then the accessibility tree's focus is the "textinput" named "Second"

    Scenario: A chord holds its modifiers while the key is pressed
      When the agent calls the tool "app_type" with:
        """json
        {"text": "select me"}
        """
      And the agent calls the tool "app_press" with:
        """json
        {"keys": "Ctrl+A"}
        """
      Then the property "selectedText" of "first" is "select me"

    Scenario: A key can be repeated
      When the agent calls the tool "app_press" with:
        """json
        {"keys": "Enter", "repeat": 3}
        """
      Then the property "enters" of "root" is 3

    Scenario: A key nobody has is refused with its name
      When the agent calls the tool "app_press" with:
        """json
        {"keys": "Hyper+Banana"}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "Banana"

  Rule: Scrolling and touching are the wheel and the finger

    Scenario: The wheel scrolls what is under it
      When the agent calls the tool "app_scroll" with:
        """json
        {"target": "#flick", "dy": -3}
        """
      Then the property "contentY" of "flick" is at least 1

    Scenario: A tap is a touch that is lifted where it began
      When the agent calls the tool "app_touch" on the reference of the "button" named "Tapper" with:
        """json
        {"action": "tap"}
        """
      Then the property "taps" of "root" is 1

    Scenario: A swipe drags what is under the finger
      When the agent calls the tool "app_touch" with:
        """json
        {"target": "#flick", "action": "swipe", "dy": -40}
        """
      Then the property "contentY" of "flick" is at least 1

    # The scene routes one finger as a pointer with an id; it has no gesture of
    # two fingers yet, so nothing can answer a pinch.
    @backlog
    Scenario: A pinch changes what a pinch handler holds
      When the agent calls the tool "app_touch" on the reference of the "button" named "Tapper" with:
        """json
        {"action": "pinch", "scale": 2}
        """
      Then the answer's "/gesture" is "pinch"

  Rule: The clock moves only when it is the agent's to move

    Scenario: Time that is advanced runs the timers it passes
      When the agent calls the tool "app_advance" with:
        """json
        {"ms": 350}
        """
      Then the property "ticks" of "root" is 3
      And the answer's "/advanced_ms" is 350

    Scenario: Time cannot be run backwards or without end
      When the agent calls the tool "app_advance" with:
        """json
        {"ms": 100000000}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "ms"

  Rule: The window and what the system prefers can be changed

    Scenario: A window of another size
      When the agent calls the tool "app_resize" with:
        """json
        {"width": 640, "height": 480, "scale": 2}
        """
      And the agent calls the tool "app_status"
      Then the answer's "/windows/0/width" is 640
      And the answer's "/windows/0/height" is 480
      And the answer's "/windows/0/scale" is 2

    Scenario: A window that cannot be is refused
      When the agent calls the tool "app_resize" with:
        """json
        {"width": 0, "height": 480}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "width"

    Scenario: The colour scheme, reduced motion and text size are what the application reads
      When the agent calls the tool "app_set_appearance" with:
        """json
        {"color_scheme": "dark", "reduced_motion": true, "font_scale": 1.5}
        """
      Then the property "scheme" of "root" is 2
      And the answer's "/appearance/color_scheme" is "dark"
      And the answer's "/appearance/reduced_motion" is true
      And the answer's "/appearance/font_scale" is 1.5

    Scenario: An appearance with nothing to change is refused
      When the agent calls the tool "app_set_appearance"
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "color_scheme"

  Rule: An argument that is wrong is told before anything is done

    Scenario: A misspelt argument is named
      When the agent calls the tool "app_click" with:
        """json
        {"target": "#cover", "cuont": 2}
        """
      Then the agent is refused with "invalid_argument"
      And the refusal mentions "cuont"
      And the property "covered" of "root" is 0
