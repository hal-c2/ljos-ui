# Sources:
#   https://developer.android.com/develop/ui/views/notifications/badges
#   https://developer.android.com/reference/android/app/NotificationChannel#setShowBadge(boolean)
#   https://developer.android.com/reference/android/app/Notification.Builder#setNumber(int)
#   https://developer.android.com/develop/ui/views/notifications/notification-permission
#   https://doc.qt.io/qt-6/qguiapplication.html#setBadgeNumber
#   docs/ARCHITECTURE.md (§10.1: the platform's services)
#   https://github.com/hal-c2/ljos-ui/issues/616
# The application's badge is a count on its icon
# (`Platform::set_badge_number`, as `QGuiApplication::setBadgeNumber`). QML
# has no member for it, as Qt's has none: the application sets it from
# Rust, and these scenarios have it set as the application starts.
# Android badges an icon only for the application's notifications, so the
# count is the number of one notification, in a channel that badges, and
# it needs the permission to show notifications
# (features/mobile/notification-permission.feature). Without that
# permission nothing is shown and nothing is said; a count set before the
# user grants it is shown once they do.
@ljos-only
Feature: The application's badge on a phone
  The application shows a count on its icon: the messages unread, say.
  On a phone the count needs the permission to show notifications, and
  is shown once the user grants it.

  Rule: A desktop shows the count as the application sets it

    Scenario: The count set as the application starts is shown
      Given the application's badge is 3 as it starts
      And the QML:
        """qml
        import QtQuick
        Item {}
        """
      Then the system shows 3 on the application's icon

  # Ljós's tests stand in for a system that shows the badge only to an
  # application that may show notifications, as Android 13 and later do.
  Rule: A system that asks the user before it shows notifications

    Background:
      Given the system asks the user before it grants a permission
      And the application's badge is 3 as it starts
      And the QML:
        """qml
        import QtQuick
        import Ljos.Platform
        Item {
          NotificationPermission { objectName: "notifications" }
        }
        """

    Scenario: The badge waits for the permission to show notifications
      Then the system shows nothing on the application's icon
      And the log has no warnings

    Scenario: The badge set before the user grants the permission shows once they do
      When the method "request" of "notifications" is called
      And the user grants the "notifications" permission
      Then the system shows 3 on the application's icon

    Scenario: The badge the user denies notifications for stays unshown and says nothing
      When the method "request" of "notifications" is called
      And the user denies the "notifications" permission
      Then the system shows nothing on the application's icon
      And the log has no warnings

  # The emulator the runner uses is Android 16 (API 36), which asks.
  @android
  Rule: On Android the badge is the number of a notification

    Background:
      Given the system asks the user before it grants a permission
      And the application's badge is 3 as it starts
      And the QML:
        """qml
        import QtQuick
        import Ljos.Platform
        Window {
          visible: true
          NotificationPermission { objectName: "notifications" }
        }
        """

    Scenario: On a phone the badge waits for the permission to show notifications
      Then the system shows nothing on the application's icon
      And the log has no warnings

    Scenario: On a phone the badge shows once the user grants the permission
      When the method "request" of "notifications" is called
      And the user grants the "notifications" permission
      Then the system shows 3 on the application's icon

    Scenario: On a phone the badge the user denies notifications for says nothing
      When the method "request" of "notifications" is called
      And the user denies the "notifications" permission
      Then the system shows nothing on the application's icon
      And the log has no warnings
