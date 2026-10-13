# Sources:
#   https://doc.qt.io/qt-6/permissions.html
#   https://doc.qt.io/qt-6/qml-qtcore-camerapermission.html
#   https://doc.qt.io/qt-6/qml-qtcore-microphonepermission.html
#   https://doc.qt.io/qt-6/qml-qtcore-locationpermission.html
#   https://doc.qt.io/qt-6/qml-qtcore-calendarpermission.html
#   https://doc.qt.io/qt-6/qml-qtcore-contactspermission.html
#   https://doc.qt.io/qt-6/qml-qtcore-bluetoothpermission.html
# Verified on Qt 6.11.2 (qml6, offscreen, desktop Linux).

Feature: The permissions of QtCore
  An application asks before it uses what the system guards: the camera,
  the microphone, the user's location, calendar and contacts, and
  Bluetooth. A permission's status says whether it may; request() asks the
  user, and the answer comes later. Desktop Linux guards none of these:
  every permission is granted from the start and asking changes nothing.

  @qt-parity
  Rule: On desktop Linux every permission is granted

    Scenario Outline: A <type> is granted from the start
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item { <type> { objectName: "p" } }
        """
      Then the property "status" of "p" is 1

      Examples:
        | type                 |
        | CameraPermission     |
        | MicrophonePermission |
        | LocationPermission   |
        | CalendarPermission   |
        | ContactsPermission   |
        | BluetoothPermission  |

    Scenario Outline: Asking for a granted <type> changes nothing
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item {
          id: root
          objectName: "root"
          property int changes: 0
          <type> { objectName: "p"; onStatusChanged: root.changes++ }
        }
        """
      When the method "request" of "p" is called
      And 100 milliseconds pass
      Then the property "status" of "p" is 1
      And the property "changes" of "root" is 0

      Examples:
        | type                 |
        | CameraPermission     |
        | MicrophonePermission |
        | LocationPermission   |
        | CalendarPermission   |
        | ContactsPermission   |
        | BluetoothPermission  |

    Scenario Outline: A <type> starts with <property> <value>
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item { <type> { objectName: "p" } }
        """
      Then the property "<property>" of "p" is <value>

      Examples:
        | type                | property           | value |
        | LocationPermission  | accuracy           | 0     |
        | LocationPermission  | availability       | 0     |
        | CalendarPermission  | accessMode         | 0     |
        | ContactsPermission  | accessMode         | 0     |
        | BluetoothPermission | communicationModes | 3     |

    Scenario Outline: <name> is <value>
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item { objectName: "o"; property var v: <name> }
        """
      Then the property "v" of "o" is <value>

      Examples:
        | name                             | value |
        | Qt.PermissionStatus.Undetermined | 0     |
        | Qt.PermissionStatus.Granted      | 1     |
        | Qt.PermissionStatus.Denied       | 2     |
        | LocationPermission.Approximate   | 0     |
        | LocationPermission.Precise       | 1     |
        | LocationPermission.WhenInUse     | 0     |
        | LocationPermission.Always        | 1     |
        | CalendarPermission.ReadOnly      | 0     |
        | CalendarPermission.ReadWrite     | 1     |
        | ContactsPermission.ReadOnly      | 0     |
        | ContactsPermission.ReadWrite     | 1     |
        | BluetoothPermission.Access       | 1     |
        | BluetoothPermission.Advertise    | 2     |
        | BluetoothPermission.Default      | 3     |

    Scenario: A precise location, always, is granted as well
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item {
          LocationPermission {
            objectName: "p"
            accuracy: LocationPermission.Precise
            availability: LocationPermission.Always
          }
        }
        """
      Then the property "status" of "p" is 1
      And the property "accuracy" of "p" is 1
      And the property "availability" of "p" is 1

    Scenario: Asking for less keeps the permission granted
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item {
          CalendarPermission { objectName: "p"; accessMode: CalendarPermission.ReadWrite }
        }
        """
      When the property "accessMode" of "p" is set to 0
      Then the property "accessMode" of "p" is 0
      And the property "status" of "p" is 1

  # Qt's permissions on desktop Linux never ask anyone. Ljós's tests stand
  # in for a system that does (macOS, Android, iOS) and answer as its user.
  # A permission is made granted and asks the system once it is complete,
  # so a system that says otherwise is heard as one statusChanged then.
  @ljos-only
  Rule: A system that asks the user before it grants

    Background:
      Given the system asks the user before it grants a permission
      And the QML:
        """qml
        import QtQuick
        import QtCore
        Item {
          id: root
          objectName: "root"
          property string log: ""
          CameraPermission {
            objectName: "camera"
            onStatusChanged: root.log += status + ";"
          }
          MicrophonePermission { objectName: "microphone" }
        }
        """

    Scenario: A permission not asked for yet is undetermined
      Then the property "status" of "camera" is 0
      And the property "log" of "root" is "0;"
      And the system asks the user for no permission

    Scenario: request() asks the user, whose answer comes later
      When the method "request" of "camera" is called
      Then the system asks the user for the "camera" permission
      And the property "status" of "camera" is 0
      And the property "log" of "root" is "0;"

    Scenario: A permission the user grants is granted
      When the method "request" of "camera" is called
      And the user grants the "camera" permission
      Then the property "status" of "camera" is 1
      And the property "log" of "root" is "0;1;"
      And the system asks the user for no permission

    Scenario: A permission the user denies is denied
      When the method "request" of "camera" is called
      And the user denies the "camera" permission
      Then the property "status" of "camera" is 2
      And the property "log" of "root" is "0;2;"

    Scenario: Each permission is asked for on its own
      When the method "request" of "microphone" is called
      And the user grants the "microphone" permission
      Then the property "status" of "microphone" is 1
      And the property "status" of "camera" is 0
