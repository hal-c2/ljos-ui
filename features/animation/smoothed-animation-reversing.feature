# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-smoothedanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: SmoothedAnimation with a new value on the way
  A new value on the way is approached from the speed the property has at that moment.
  reversingMode says what happens when the new value lies the other way.

  # Qt runs on real time here, so values on the way are given with a tolerance. Each property is
  # at 50 and moving up at 200 per second when its new value comes.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real eased: 0
        property real immediate: 0
        property real sync: 0
        Behavior on eased { SmoothedAnimation { velocity: 100 } }
        Behavior on immediate {
          SmoothedAnimation { velocity: 100; reversingMode: SmoothedAnimation.Immediate }
        }
        Behavior on sync {
          SmoothedAnimation { velocity: 100; reversingMode: SmoothedAnimation.Sync }
        }
      }
      """

  Scenario: A value further the same way is approached without slowing down
    When the property "eased" of "root" is set to 100
    And 500 milliseconds pass
    And the property "eased" of "root" is set to 200
    And 100 milliseconds pass
    Then the property "eased" of "root" is within 8 of 70
    When 3 seconds pass
    Then the property "eased" of "root" is 200

  Scenario: Eased, the property overshoots before it turns round
    When the property "eased" of "root" is set to 100
    And 500 milliseconds pass
    And the property "eased" of "root" is set to 0
    And 100 milliseconds pass
    Then the property "eased" of "root" is within 8 of 61.5
    When 2 seconds pass
    Then the property "eased" of "root" is 0

  Scenario: Immediate, the property turns round at once
    When the property "immediate" of "root" is set to 100
    And 500 milliseconds pass
    And the property "immediate" of "root" is set to 0
    And 100 milliseconds pass
    Then the property "immediate" of "root" is within 8 of 46
    When 2 seconds pass
    Then the property "immediate" of "root" is 0

  Scenario: Sync, the property takes the new value at once
    When the property "sync" of "root" is set to 100
    And 500 milliseconds pass
    And the property "sync" of "root" is set to 0
    Then the property "sync" of "root" is 0

  Scenario: Sync, a value the same way is still approached smoothly
    When the property "sync" of "root" is set to 100
    And 500 milliseconds pass
    And the property "sync" of "root" is set to 200
    And 100 milliseconds pass
    Then the property "sync" of "root" is within 8 of 70
