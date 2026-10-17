# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html#delegate-prop
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#required-properties
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Where a delegate that cannot be made is told
  A delegate whose object cannot be made, such as one with a required
  property its row does not give, is told at the delegate's component: the
  object written as the delegate, or the Component it names. What stopped
  it is told after, at its own place.

  Scenario Outline: A delegate written as <delegate> is told at <at>
    Given the file "Needy.qml":
      """qml
      import QtQuick
      Item {
        required property int nope
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        Component { id: comp; Item { required property int c } }
        <view> {
          model: 1
          <delegate>
        }
      }
      """
    When 100 milliseconds pass
    Then the log has an info matching "Scenario\.qml:<at>: QML Component: Cannot create delegate$"
    And the log has a warning matching "<file>\.qml:<where>: Required property <name> was not initialized$"

    Examples:
      | view     | delegate                                   | at   | file     | where | name |
      | Repeater | delegate: Item { required property int a } | 6:21 | Scenario | 6:28  | a    |
      | Repeater | Item { required property int b }           | 6:11 | Scenario | 6:18  | b    |
      | Repeater | delegate: comp                             | 3:9  | Scenario | 3:38  | c    |
      | Repeater | delegate: Needy {}                         | 6:21 | Needy    | 3:9   | nope |
      | ListView | delegate: Item { required property int a } | 6:21 | Scenario | 6:28  | a    |
