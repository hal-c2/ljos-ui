# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#opacity-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-slider.html#value-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-spinbox.html#value-prop
#   https://doc.qt.io/qt-6/properties.html (a property's WRITE function and NOTIFY signal)
# Verified on Qt 6.11.2 (qml6, offscreen).
# A write the type clamps to the value the property already has is not
# heard by any change handler.

@qt-parity
Feature: A type clamps what is written to its properties before it is stored
  A type can keep a property in range: what is written is first made to
  fit, and only then stored. A change handler hears the stored value, and
  hears nothing at all when the value that fits is the one the property
  already had.

  Scenario: Opacity above one is stored as one, and nothing is heard
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int heard: 0
        onOpacityChanged: heard++
        function brighten() { opacity = 3 }
      }
      """
    When the method "brighten" of "root" is called
    Then the property "opacity" of "root" is 1
    And the property "heard" of "root" is 0

  Scenario: Opacity below zero is stored as zero, and heard once
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int heard: 0
        property real seen: -5
        onOpacityChanged: { heard++; seen = opacity }
        function hide() { opacity = -1 }
      }
      """
    When the method "hide" of "root" is called
    Then the property "opacity" of "root" is 0
    And the property "heard" of "root" is 1
    And the property "seen" of "root" is 0

  # The binding's first value, 0.5, is heard as the item is made; 4 is
  # stored as 1 and heard; 9 is stored as 1 again and is not.
  Scenario: A binding's value is clamped too
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property real wanted: 0.5
        property int heard: 0
        opacity: wanted
        onOpacityChanged: heard++
        function overshoot() { wanted = 4 }
        function overshootMore() { wanted = 9 }
      }
      """
    When the method "overshoot" of "root" is called
    And the method "overshootMore" of "root" is called
    Then the property "opacity" of "root" is 1
    And the property "heard" of "root" is 2

  # The range controls still bring the value back after it is stored, so
  # each overshoot is heard twice; their value is to move to a setter
  # that clamps once the range is whole.
  @backlog
  Scenario Outline: A range control keeps its value between from and to
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      <type> {
        objectName: "c"
        from: 0; to: 10; value: 5
        property int heard: 0
        onValueChanged: heard++
        function overshoot() { value = 20; value = 30 }
      }
      """
    When the method "overshoot" of "c" is called
    Then the property "value" of "c" is 10
    And the property "heard" of "c" is 1

    Examples:
      | type        |
      | Slider      |
      | SpinBox     |
      | ProgressBar |
