# Sources:
#   docs/ARCHITECTURE.md (§19.4: the inspector; §19.7: transports, Android)
#   docs/research/10-mcp.md (Mobile: adb forward, the token read with run-as)
#   https://developer.android.com/tools/adb#forwardports
#   https://developer.android.com/tools/adb#shellcommands (run-as)
#   https://github.com/hal-c2/ljos-ui/issues/617
#   https://github.com/hal-c2/ljos-ui/issues/825
# The inspector of an application on a phone listens on the phone's own
# loopback address and writes its token file in the application's own
# directory. `ljos android run --mcp=…` starts a debug build with it;
# `ljos android connect` forwards its port, the same on the computer, to it over adb,
# reads the token with `run-as` (which only a debuggable package allows),
# and writes a token file on the computer that `ljos observe`,
# `ljos attach` and agents take. `ljos android disconnect` removes that
# forward again: adb keeps one until it restarts, and a port forwarded for
# any phone refuses an inspector at that port on another (#822). A connect
# to an inspector at another port removes the forward of the connect
# before it, unless the inspector that forward reaches refuses that
# connect's token: another application's connect took it (#825).
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
        Text {
          objectName: "hello"
          text: "Hello from the phone"
          Accessible.role: Accessible.StaticText
        }
      }
      """

  Scenario: The agent sees the scene of the application on the phone
    Given the inspector of the application on the phone is connected over adb
    When the agent requests a snapshot
    Then the snapshot contains "- label \"Hello from the phone\" [e2] obj=hello"

  Scenario: The port forwarded to the inspector goes when the developer disconnects
    Given the inspector of the application on the phone is connected over adb
    When the developer disconnects from the inspector on the phone
    Then no port of this computer is forwarded to the inspector on the phone

  Scenario: The port forwarded to the inspector before goes when the developer connects to the application started anew
    Given the inspector of the application on the phone is connected over adb
    When the developer connects to the inspector of the application started anew
    Then the port forwarded to the inspector before is forwarded no more
