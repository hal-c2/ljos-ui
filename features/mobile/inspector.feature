# Sources:
#   docs/ARCHITECTURE.md (§19.4: the inspector; §19.7: transports, Android)
#   docs/research/10-mcp.md (Mobile: adb forward, the token read with run-as)
#   https://developer.android.com/tools/adb#forwardports
#   https://developer.android.com/tools/adb#shellcommands (run-as)
#   https://github.com/hal-c2/ljos-ui/issues/617
# The inspector of an application on a phone listens on the phone's own
# loopback address and writes its token file in the application's own
# directory. `ljos android run --mcp=…` starts a debug build with it;
# `ljos android connect` forwards a port of the computer to it over adb,
# reads the token with `run-as` (which only a debuggable package allows),
# and writes a token file on the computer that `ljos observe`,
# `ljos attach` and agents take.
@ljos-only @android
Feature: An agent inspects an application running on a phone
  The developer starts the application on the phone with the inspector
  and connects to it from the computer; an agent there then sees the
  application's scene as it would one on the desktop.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        visible: true
        Text { objectName: "hello"; text: "Hello from the phone" }
      }
      """

  Scenario: The agent sees the scene of the application on the phone
    Given the inspector of the application on the phone is connected over adb
    When the agent requests a snapshot
    Then the snapshot has a "Text" named "hello" with a reference
