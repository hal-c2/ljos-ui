# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-styles.html#using-styles-in-qt-quick-controls
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html#creating-a-custom-style
#   https://doc.qt.io/qt-6/qtquickcontrols-environment.html
#   https://doc.qt.io/qt-6/qquickstyle.html
# Verified on Qt 6.11.2 (qml6, offscreen, QT_QUICK_CONTROLS_STYLE set): a
# custom style's own controls come first and the others are Basic's. With
# nothing chosen qml6 on Linux has Fusion; "-style" on the command line
# comes before the variable, and the variable before the file.

@qt-parity
Feature: Choosing the style when the application starts
  A program that imports QtQuick.Controls gets the style chosen when it
  starts, by QT_QUICK_CONTROLS_STYLE or a qtquickcontrols2.conf file: a
  built-in style, or a module of the application's own that is a custom
  style. A custom style need not have every control: the ones it lacks
  come from its fallback style, which is Basic unless another is named.
  A program that chooses none has its platform's style, Fusion on Linux;
  "-style" on its command line chooses one too, before anything else.

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

  # The platform's style is Fusion here: the scenarios run on Linux.
  Scenario: A program that chooses no style has the platform's
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Button { objectName: "button"; text: "Go" }
      """
    When 100 milliseconds pass
    Then the property "implicitHeight" of "button" is 24
    And the property "palette.button" of "button" is "#efefef"

  Scenario Outline: A style chosen by <how> comes before the platform's
    Given <choice>
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Button { objectName: "button"; text: "Go" }
      """
    When 100 milliseconds pass
    Then the property "implicitHeight" of "button" is 40

    Examples:
      | how              | choice                                                                    |
      | the variable     | the application starts with "QT_QUICK_CONTROLS_STYLE" set to "Basic"      |
      | the command line | the application starts with "-style Basic" on its command line            |

  Scenario: A style chosen by the configuration file comes before the platform's
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Basic
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Button { objectName: "button"; text: "Go" }
      """
    When 100 milliseconds pass
    Then the property "implicitHeight" of "button" is 40

  Scenario: A program that imports a style has it, whatever the platform's
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Button { objectName: "button"; text: "Go" }
      """
    When 100 milliseconds pass
    Then the property "implicitHeight" of "button" is 40

  Scenario: The command line comes before the variable
    Given the application starts with "QT_QUICK_CONTROLS_STYLE" set to "Material"
    And the application starts with "-style Basic" on its command line
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Button { objectName: "button"; text: "Go" }
      """
    When 100 milliseconds pass
    Then the property "implicitHeight" of "button" is 40

  Scenario: The variable comes before the configuration file
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Material
      """
    And the application starts with "QT_QUICK_CONTROLS_STYLE" set to "Basic"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Button { objectName: "button"; text: "Go" }
      """
    When 100 milliseconds pass
    Then the property "implicitHeight" of "button" is 40
