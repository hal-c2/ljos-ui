# Sources:
#   https://developer.android.com/develop/ui/views/notifications/notification-permission
#   https://developer.android.com/reference/android/Manifest.permission#POST_NOTIFICATIONS
#   https://developer.android.com/reference/android/app/NotificationManager#areNotificationsEnabled()
#   https://doc.qt.io/qt-6/permissions.html (the shape of a permission: status, request())
#   docs/DIALECT.md (§4.1: `Ljos.Platform` holds the notifications)
#   docs/ARCHITECTURE.md (§10.1: the platform's services)
#   https://github.com/hal-c2/ljos-ui/issues/616
# QtCore has no permission for notifications, so this one is Ljós's own,
# in `Ljos.Platform`, with the shape of QtCore's: a status, request(), and
# statusChanged. On Android 13 (API 33) and later it is the runtime
# permission POST_NOTIFICATIONS; before, the system grants it at install.
# A desktop guards nothing here, as with QtCore's permissions on desktop
# Linux. features/mobile/permissions.feature pins the phone's question for
# QtCore's permissions; the scenarios' application declares
# POST_NOTIFICATIONS too.
@ljos-only
Feature: The permission to show notifications
  An application asks before it shows the user a notification. The
  permission's status says whether it may; request() asks the user, and
  the answer comes later. A desktop grants it from the start; a phone
  asks.

  Rule: A desktop grants the permission to show notifications

    Scenario: The notification permission is granted from the start
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Platform
        Item { NotificationPermission { objectName: "p" } }
        """
      Then the property "status" of "p" is 1

    Scenario: Asking for a granted notification permission changes nothing
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Platform
        Item {
          id: root
          objectName: "root"
          property int changes: 0
          NotificationPermission { objectName: "p"; onStatusChanged: root.changes++ }
        }
        """
      When the method "request" of "p" is called
      And 100 milliseconds pass
      Then the property "status" of "p" is 1
      And the property "changes" of "root" is 0

    Scenario: A file of Ljós's API reads the status as Permission.Status
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Platform
        Item {
          objectName: "root"
          NotificationPermission { id: p }
          property bool granted: p.status === Permission.Status.Granted
        }
        """
      Then the property "granted" of "root" is true

  # Ljós's tests stand in for a system that asks (Android 13 and later)
  # and answer as its user.
  Rule: A system that asks the user before it shows notifications

    Background:
      Given the system asks the user before it grants a permission
      And the QML:
        """qml
        import QtQuick
        import QtCore
        import Ljos.Platform
        Item {
          id: root
          objectName: "root"
          property string log: ""
          NotificationPermission {
            objectName: "notifications"
            onStatusChanged: root.log += status + ";"
          }
          CameraPermission { objectName: "camera" }
        }
        """

    Scenario: The notification permission not asked for yet is undetermined
      Then the property "status" of "notifications" is 0
      And the property "log" of "root" is "0;"
      And the system asks the user for no permission

    Scenario: request() asks the user for the notification permission
      When the method "request" of "notifications" is called
      Then the system asks the user for the "notifications" permission
      And the property "status" of "notifications" is 0

    Scenario: The notification permission the user grants is granted
      When the method "request" of "notifications" is called
      And the user grants the "notifications" permission
      Then the property "status" of "notifications" is 1
      And the property "log" of "root" is "0;1;"
      And the property "status" of "camera" is 0

    Scenario: The notification permission the user denies is denied
      When the method "request" of "notifications" is called
      And the user denies the "notifications" permission
      Then the property "status" of "notifications" is 2
      And the property "log" of "root" is "0;2;"

  # The emulator the runner uses is Android 16 (API 36), which asks.
  @android
  Rule: On Android 13 and later the system asks the user

    Background:
      Given the system asks the user before it grants a permission
      And the QML:
        """qml
        import QtQuick
        import Ljos.Platform
        Window {
          id: root
          objectName: "root"
          visible: true
          property string log: ""
          NotificationPermission {
            objectName: "notifications"
            onStatusChanged: root.log += status + ";"
          }
        }
        """

    Scenario: On a phone the notification permission not asked for yet is undetermined
      Then the property "status" of "notifications" is 0
      And the system asks the user for no permission

    Scenario: On a phone request() shows the system's question
      When the method "request" of "notifications" is called
      Then the system asks the user for the "notifications" permission
      And the property "status" of "notifications" is 0

    Scenario: On a phone the notification permission the user grants is granted
      When the method "request" of "notifications" is called
      And the user grants the "notifications" permission
      Then the property "status" of "notifications" is 1
      And the property "log" of "root" is "0;1;"

    Scenario: On a phone the notification permission the user denies is denied
      When the method "request" of "notifications" is called
      And the user denies the "notifications" permission
      Then the property "status" of "notifications" is 2
