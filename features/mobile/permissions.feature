# Sources:
#   https://doc.qt.io/qt-6/permissions.html
#   https://doc.qt.io/qt-6/qml-qtcore-camerapermission.html
#   https://doc.qt.io/qt-6/qml-qtcore-microphonepermission.html
#   https://developer.android.com/training/permissions/requesting
#   https://developer.android.com/reference/android/app/Activity#requestPermissions(java.lang.String[],%20int)
#   https://developer.android.com/reference/android/Manifest.permission
#   docs/ARCHITECTURE.md (§10.1: the platform's services)
#   https://github.com/hal-c2/ljos-ui/issues/616
# On a phone a permission of QtCore is one or more of Android's runtime
# permissions, and request() shows the system's own question.
# features/platform/permissions.feature pins the same behaviour with a
# stand-in system. The scenarios' application declares the camera and the
# microphone ([package.metadata.ljos.android] permissions), and each
# scenario starts with it never asked for.
@ljos-only @android
Feature: A permission on a phone is the system's
  The application asks, and the system asks the user. A permission the
  system has not granted is undetermined until the application has asked
  for it, and denied once the user said no. One the application's
  manifest does not declare is denied, and nobody is asked.

  Background:
    Given the system asks the user before it grants a permission
    And the QML:
      """qml
      import QtQuick
      import QtCore
      Window {
        id: root
        objectName: "root"
        visible: true
        property string log: ""
        CameraPermission {
          objectName: "camera"
          onStatusChanged: root.log += status + ";"
        }
        MicrophonePermission { objectName: "microphone" }
        ContactsPermission { objectName: "contacts" }
      }
      """

  Scenario: A permission not asked for yet is undetermined
    Then the property "status" of "camera" is 0
    And the property "status" of "microphone" is 0
    And the system asks the user for no permission

  Scenario: request() shows the system's question
    When the method "request" of "camera" is called
    Then the system asks the user for the "camera" permission
    And the property "status" of "camera" is 0

  Scenario: A permission the user grants is granted
    When the method "request" of "camera" is called
    And the user grants the "camera" permission
    Then the property "status" of "camera" is 1
    And the property "log" of "root" is "0;1;"
    And the property "status" of "microphone" is 0

  Scenario: A permission the user denies is denied
    When the method "request" of "microphone" is called
    And the user denies the "microphone" permission
    Then the property "status" of "microphone" is 2

  Scenario: A permission the manifest does not declare is denied without asking
    Then the property "status" of "contacts" is 2
    When the method "request" of "contacts" is called
    Then the system asks the user for no permission
    And the property "status" of "contacts" is 2
