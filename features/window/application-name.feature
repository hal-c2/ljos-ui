# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-application.html
#   https://doc.qt.io/qt-6/qcoreapplication.html#applicationName-prop
#   https://doc.qt.io/qt-6/qguiapplication.html#applicationDisplayName-prop
# Verified on Qt 6.11.2 (qml6, offscreen). The oracle's program names
# itself, so the scenarios check only that the name is not empty.

@qt-parity
Feature: The application's name
  An application has a name before anything gives it one, the program's.
  Its display name is its name until something gives it a display name of
  its own, even an empty one.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool named: Application.name.length > 0
        property bool shown: Application.displayName === Application.name
        property string displayName: Application.displayName
        function rename(n) { Qt.application.name = n }
        function show(n) { Qt.application.displayName = n }
      }
      """

  @api-QtQuick.Application.name @api-QtQuick.Application.displayName
  Scenario: An application nothing named has the program's name, and shows it
    Then the property "named" of "root" is true
    And the property "shown" of "root" is true

  @api-QtQuick.Application.displayName
  Scenario: The display name follows the name
    When the method "rename" of "root" is called with "Notes"
    Then the property "displayName" of "root" is "Notes"

  @api-QtQuick.Application.displayName
  Scenario: A display name of its own stays when the name changes
    When the method "show" of "root" is called with "My Notes"
    And the method "rename" of "root" is called with "Other"
    Then the property "displayName" of "root" is "My Notes"

  @api-QtQuick.Application.displayName
  Scenario: An empty display name is one of its own
    When the method "show" of "root" is called with ""
    And the method "rename" of "root" is called with "Other"
    Then the property "displayName" of "root" is ""
