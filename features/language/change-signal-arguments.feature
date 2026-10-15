# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html#property-change-signal-handlers
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#text-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: What a change signal carries
  Some of a type's change signals carry the new value: Text's
  textChanged(text). A handler of one sees the value by that name.

  Scenario: A Text's change handler sees the new text by its name
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string seen: ""
        Text { objectName: "label"; onTextChanged: seen += "[" + text + "]" }
      }
      """
    When the property "text" of "label" is set to "hello"
    Then the property "seen" of "root" is "[hello]"

  Scenario: A handler of a property the QML declares sees it as the property
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int count: 0
        property string seen: ""
        onCountChanged: seen += "[" + count + "]"
      }
      """
    When the property "count" of "root" is set to 3
    Then the property "seen" of "root" is "[3]"
