# Sources:
#   https://doc.qt.io/qt-6/qnetworkinformation.html
#   https://doc.qt.io/qt-6/qml-qtnetwork-networkinformation.html
#   https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.NetworkMonitor.html
#   https://networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMConnectivityState
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: An application knows whether the system can reach the network
  NetworkInformation (QtNetwork) says what the system says of its network: how far it reaches
  (`reachability`: Unknown, Disconnected, Local, Site or Online), whether a captive portal is in
  the way, which medium carries it and whether it is metered. The host reports it when the
  application starts and each time the system says it changed, and what read it follows. A host
  with nothing to ask says Unknown, so an application can tell "offline" from "can't tell".

  @qt-parity
  Scenario: The reachability and the transport medium are enums of NetworkInformation
    Given the QML:
      """qml
      import QtQuick
      import QtNetwork
      Item {
        objectName: "root"
        property var reachability: [NetworkInformation.Reachability.Unknown,
          NetworkInformation.Reachability.Disconnected, NetworkInformation.Reachability.Local,
          NetworkInformation.Reachability.Site, NetworkInformation.Reachability.Online].join(" ")
        property var media: [NetworkInformation.TransportMedium.Unknown,
          NetworkInformation.TransportMedium.Ethernet, NetworkInformation.TransportMedium.Cellular,
          NetworkInformation.TransportMedium.WiFi, NetworkInformation.TransportMedium.Bluetooth].join(" ")
        property string kind: typeof NetworkInformation.reachability
      }
      """
    Then the property "reachability" of "root" is "0 1 2 3 4"
    And the property "media" of "root" is "0 1 2 3 4"
    And the property "kind" of "root" is "number"

  @qt-parity
  Scenario: The reachability is the system's to say
    Given the QML:
      """qml
      import QtQuick
      import QtNetwork
      Item {
        objectName: "root"
        property string error
        property bool unchanged
        Component.onCompleted: {
          const before = NetworkInformation.reachability
          const other = before === 1 ? 2 : 1
          try { NetworkInformation.reachability = other } catch (e) { error = e.name }
          unchanged = NetworkInformation.reachability === before
        }
      }
      """
    Then the property "error" of "root" is "TypeError"
    And the property "unchanged" of "root" is true

  # The oracle's network is the machine's.
  @ljos-only
  Scenario: Until the system says, the reachability is unknown
    Given the QML:
      """qml
      import QtQuick
      import QtNetwork
      Item {
        objectName: "root"
        property int reachability: NetworkInformation.reachability
        property int medium: NetworkInformation.transportMedium
        property bool portal: NetworkInformation.isBehindCaptivePortal
        property bool metered: NetworkInformation.isMetered
      }
      """
    Then the property "reachability" of "root" is 0
    And the property "medium" of "root" is 0
    And the property "portal" of "root" is false
    And the property "metered" of "root" is false

  @ljos-only
  Scenario Outline: What the system says of its network when the application starts
    Given the system's network is "<network>"
    And the QML:
      """qml
      import QtQuick
      import QtNetwork
      Item {
        objectName: "root"
        property int reachability: NetworkInformation.reachability
        property int medium: NetworkInformation.transportMedium
        property bool portal: NetworkInformation.isBehindCaptivePortal
        property bool metered: NetworkInformation.isMetered
      }
      """
    Then the property "reachability" of "root" is <reachability>
    And the property "medium" of "root" is <medium>
    And the property "portal" of "root" is <portal>
    And the property "metered" of "root" is <metered>

    Examples:
      | network                         | reachability | medium | portal | metered |
      | online                          | 4            | 0      | false  | false   |
      | online, wifi                    | 4            | 3      | false  | false   |
      | online, cellular, metered       | 4            | 2      | false  | true    |
      | site, ethernet                  | 3            | 1      | false  | false   |
      | site, behind a captive portal   | 3            | 0      | true   | false   |
      | local                           | 2            | 0      | false  | false   |
      | disconnected                    | 1            | 0      | false  | false   |
      | unknown                         | 0            | 0      | false  | false   |

  @ljos-only
  Scenario: An application follows the network as it goes and comes back
    Given the system's network is "online, wifi"
    And the QML:
      """qml
      import QtQuick
      import QtNetwork
      Item {
        id: root
        objectName: "root"
        property bool online: NetworkInformation.reachability === NetworkInformation.Reachability.Online
        property string status: online ? "online" : "offline"
        property int changes: 0
        Connections {
          target: NetworkInformation
          function onReachabilityChanged() { root.changes++ }
        }
      }
      """
    When the system's network changes to "disconnected"
    Then the property "status" of "root" is "offline"
    And the property "changes" of "root" is 1
    When the system's network changes to "disconnected"
    Then the property "changes" of "root" is 1
    When the system's network changes to "online, ethernet"
    Then the property "status" of "root" is "online"
    And the property "changes" of "root" is 2
