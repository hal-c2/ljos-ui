# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-actiongroup.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-action.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ActionGroup that is not exclusive
  A group told it is not exclusive lets each of its actions be checked on
  its own, and it still disables and enables its actions.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        property bool want: true
        ActionGroup { id: loose; objectName: "loose"; exclusive: false }
        Action { objectName: "n1"; checkable: true; ActionGroup.group: loose; enabled: root.want }
        Action { objectName: "n2"; checkable: true; ActionGroup.group: loose }
      }
      """

  @api-QtQuick.Controls.ActionGroup.enabled
  Scenario: A binding on the enabled of an action shows again when its group is enabled
    When the property "enabled" of "loose" is set to false
    And the property "want" of "root" is set to false
    And the property "want" of "root" is set to true
    Then the property "enabled" of "n1" is false
    When the property "enabled" of "loose" is set to true
    Then the property "enabled" of "n1" is true
    When the property "want" of "root" is set to false
    Then the property "enabled" of "n1" is false

  @api-QtQuick.Controls.ActionGroup.exclusive
  Scenario: In a group that is not exclusive, actions are checked on their own
    When the method "trigger" of "n1" is called
    And the method "trigger" of "n2" is called
    Then the property "checked" of "n1" is true
    And the property "checked" of "n2" is true
    And the property "checkedAction" of "loose" is null
