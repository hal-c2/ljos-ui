# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-styles.html#using-styles-in-qt-quick-controls
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html#creating-a-custom-style
#   https://doc.qt.io/qt-6/qtquickcontrols-environment.html
# Verified on Qt 6.11.2 (qml6, offscreen, QT_QUICK_CONTROLS_STYLE set): a
# custom style's own controls come first and the others are Basic's.

@qt-parity
Feature: Choosing the style when the application starts
  A program that imports QtQuick.Controls gets the style chosen when it
  starts, by QT_QUICK_CONTROLS_STYLE or a qtquickcontrols2.conf file: a
  built-in style, or a module of the application's own that is a custom
  style. A custom style need not have every control: the ones it lacks
  come from its fallback style, which is Basic unless another is named.

  Background:
    Given the file "Kit/qmldir":
      """
      module Kit
      Button 1.0 Button.qml
      """
    And the file "Kit/Button.qml":
      """qml
      import QtQuick
      import QtQuick.Templates as T
      T.Button {
        implicitWidth: 123
        implicitHeight: 45
        property string madeBy: "Kit"
      }
      """

  Scenario: A custom style's controls are the ones QtQuick.Controls gives
    Given the application starts with the controls style "Kit"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        Button { objectName: "button"; text: "Go" }
        TextField { objectName: "field" }
      }
      """
    When 100 milliseconds pass
    Then the property "madeBy" of "button" is "Kit"
    And the property "implicitWidth" of "button" is 123
    And the property "implicitHeight" of "button" is 45
    And the property "implicitHeight" of "field" is 40

  Scenario: A built-in style can be chosen by name
    Given the application starts with the controls style "Material"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        Button { objectName: "button"; text: "Go" }
        TextField { objectName: "field" }
      }
      """
    When 100 milliseconds pass
    Then the property "implicitHeight" of "button" is 52
    And the property "implicitHeight" of "field" is 56
