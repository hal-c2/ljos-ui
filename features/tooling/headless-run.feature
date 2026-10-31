# Sources:
#   docs/ARCHITECTURE.md sections 12.1 (hot reload), 19.4 (the inspector's
#   tools), 19.7 (`ljos attach`) and principle 6 (0 % CPU when idle)
#   https://doc.qt.io/qt-6/qpa.html (the `offscreen` platform plugin)
# `ljos run --headless` is a Ljós tool. Qt's nearest is `qml6 -platform
# offscreen`, which has no inspector to compare with.

@ljos-only @headless
Feature: An application run with no window
  `ljos run FILE --headless` runs the application as `ljos run` does, but
  opens no window and needs no display: neither `WAYLAND_DISPLAY` nor
  `DISPLAY` is asked for. The root is laid out at `--size`, else at its own
  size, as `ljos run` and `ljos render` lay it out. An agent reaches it
  through the inspector `--mcp` serves, found from the line `ljos run`
  writes to its errors, and every tool works as it does on a window: a
  screenshot is the scene's picture, input acts,
  `--watch` applies saves and keeps what the application did, and
  `app_reload` reloads. Timers and animations run on the real clock, and
  when nothing is due the process sleeps. A run with neither `--mcp` nor
  `--watch` runs too, until the application quits: a script that only does
  its work and quits needs nobody to look at it.

  Background:
    Given the file "Main.qml":
      """qml
      import QtQuick

      Rectangle {
          objectName: "root"
          width: 200; height: 100
          color: "#3366cc"
          property int ticks: 0

          MouseArea {
              objectName: "area"
              property int clicks: 0
              anchors.fill: parent
              onClicked: clicks += 1
          }
          Timer {
              interval: 100; repeat: true; running: true
              onTriggered: parent.ticks += 1
          }
          Rectangle {
              objectName: "slider"
              width: 10; height: 10
              NumberAnimation on x { from: 0; to: 100; duration: 300 }
          }
      }
      """

  Rule: It needs no display and is as large as the root says

    Scenario: The inspector is found from the line on the errors
      When the author starts "ljos run Main.qml --headless --mcp=inspect"
      Then the application's errors mention "ljos: inspector (--mcp=inspect) at"
      And the application's errors mention "target/ljos/mcp-"

    Scenario: The picture is the root at its own size
      When the author starts "ljos run Main.qml --headless --mcp=inspect"
      And the agent calls the tool "app_screenshot"
      Then the answer has a PNG picture 200 by 100 pixels
      And the pixel at 150, 80 of the picture is "#3366cc"

    Scenario: The picture is as large as --size says
      When the author starts "ljos run Main.qml --headless --size 320x240 --mcp=inspect"
      And the agent calls the tool "app_screenshot"
      Then the answer has a PNG picture 320 by 240 pixels

    Scenario: A window root is an item of the size it says
      Given the file "Main.qml":
        """qml
        import QtQuick
        Window {
            width: 320; height: 240; visible: true
            color: "#00ff00"
        }
        """
      When the author starts "ljos run Main.qml --headless --mcp=inspect"
      And the agent calls the tool "app_screenshot"
      Then the answer has a PNG picture 320 by 240 pixels
      And the pixel at 300, 200 of the picture is "#00ff00"

  Rule: The tools act as they do on a window

    Scenario: A click reaches the item under it
      When the author starts "ljos run Main.qml --headless --mcp=inspect,input,eval"
      And the agent calls the tool "app_click" with:
        """json
        {"target": "#area"}
        """
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#area", "source": "clicks"}
        """
      Then the answer's "/value" is 1

    Scenario: A resize is the size of the next picture
      When the author starts "ljos run Main.qml --headless --mcp=inspect,input"
      And the agent calls the tool "app_resize" with:
        """json
        {"width": 640, "height": 480}
        """
      And the agent calls the tool "app_screenshot"
      Then the answer has a PNG picture 640 by 480 pixels

    # As on a window, the clock is the system's: only an application on a
    # virtual clock (a test) has one an agent can move.
    Scenario: The clock is the system's, as on a window
      When the author starts "ljos run Main.qml --headless --mcp=inspect,input"
      And the agent calls the tool "app_advance" with:
        """json
        {"ms": 5000}
        """
      Then the agent is refused with "unavailable"

    Scenario: Nothing to reload is told as that
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect,mutate"
      And the agent calls the tool "app_reload"
      Then the answer's "/outcome" is "unchanged"

  # A Text is a node of the outline when it asks to be one, as on a window
  # (features/a11y/text.feature): "plain" asks for nothing.
  Rule: The outline is the one a window's inspector answers

    Background:
      Given the file "Main.qml":
        """qml
        import QtQuick
        Column {
            Rectangle { width: 10; height: 10 }
            Text {
                objectName: "status"
                text: "Stopped"
                Accessible.role: Accessible.StaticText
            }
            Text { objectName: "plain"; text: "Plain" }
        }
        """

    Scenario: The snapshot lists a text by what it says
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect --size 200x100"
      And the agent calls the tool "app_snapshot"
      Then the answer's text contains "- label \"Stopped\""
      And the answer does not contain "Plain"

    Scenario: An item is found by its role and name
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect --size 200x100"
      And the agent calls the tool "app_find" with:
        """json
        {"locator": "role=label[name=\"Stopped\"]"}
        """
      Then the answer's "/total" is 1
      And the answer's "/matches/0/obj" is "status"

    Scenario: The item at a point has the role and name of its line
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect --size 200x100"
      And the agent calls the tool "app_find" with:
        """json
        {"locator": "at=5,15"}
        """
      Then the answer's "/matches/0/obj" is "status"
      And the answer's "/matches/0/role" is "label"
      And the answer's "/matches/0/name" is "Stopped"

    Scenario: A text with no node is found at its point with no role
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect --size 200x100"
      And the agent calls the tool "app_find" with:
        """json
        {"locator": "at=5,30"}
        """
      Then the answer's "/matches/0/obj" is "plain"
      And the answer's "/matches/0/role" is null
      And the answer's "/matches/0/name" is null

  Rule: Saves are applied as they are on a window

    Scenario: A save is applied and what the application did is kept
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect,input,eval"
      And the agent calls the tool "app_click" with:
        """json
        {"target": "#area"}
        """
      And the file "Main.qml" is changed to:
        """qml
        import QtQuick

        Rectangle {
            objectName: "root"
            width: 200; height: 100
            color: "#cc3333"

            MouseArea {
                objectName: "area"
                property int clicks: 0
                anchors.fill: parent
                onClicked: clicks += 1
            }
        }
        """
      And the agent calls the tool "app_status" with:
        """json
        {"after_reload": 0, "wait_ms": 20000}
        """
      Then the answer's "/reload/generation" is 1
      And the answer's "/reload/last/outcome" is "applied"
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "#area", "source": "clicks"}
        """
      Then the answer's "/value" is 1
      When the agent calls the tool "app_screenshot"
      Then the pixel at 150, 80 of the picture is "#cc3333"

  Rule: Time runs on the real clock, and nothing runs when nothing is due

    Scenario: Timers and animations run while nobody asks
      When the author starts "ljos run Main.qml --headless --mcp=inspect,eval"
      And 1000 milliseconds pass
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#root", "source": "ticks >= 5"}
        """
      Then the answer's "/value" is true
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "#slider", "source": "x"}
        """
      Then the answer's "/value" is 100

    Scenario: An application at rest sleeps
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
            width: 200; height: 100; color: "white"
            Text { text: "at rest" }
        }
        """
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect"
      And 1500 milliseconds pass
      And the wakes of the application's threads are counted
      And 2000 milliseconds pass
      Then the window's thread woke at most 0 times since the wakes were counted
      And no thread but the window's woke since they were counted

  Rule: A run nobody looks at runs until the application quits

    Scenario: A script that quits ends the run
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            Timer {
                interval: 50; running: true
                onTriggered: {
                    console.log("done at", width, "by", height)
                    Qt.quit()
                }
            }
            width: 120; height: 90
        }
        """
      When the author runs "ljos run Main.qml --headless"
      Then the command exits with status 0
      And the command's output mentions "done at 120 by 90"

  Rule: A run can end with the program that started it

    # A program that was killed runs nothing more: the one thing the system
    # still does for it is close its end of the pipe on the child's input.
    Scenario: Its input closing ends a run that would go on
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            width: 120; height: 90
            Timer { interval: 50; running: true; repeat: true }
        }
        """
      When the author runs "ljos run Main.qml --headless --quit-with-stdin"
      Then the command exits with status 0
