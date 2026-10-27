# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-material.html#theme-attached-prop
#   https://doc.qt.io/qt-6/qtquickcontrols-universal.html#theme-attached-prop
#   https://doc.qt.io/qt-6/qstylehints.html#colorScheme-prop
# Verified on Qt 6.11.2 (qml6, offscreen): its platform is always light,
# and an application's own scheme does not reach it.

Feature: The System theme of the Material and Universal styles
  Material.System and Universal.System are the theme of the platform's
  colour scheme: Dark while it is dark, Light otherwise. The attached
  `theme` reads the theme it gave, not System, and an item inside has it.

  @qt-parity
  Scenario Outline: System is Light in a light scheme in <style>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.<style>
      Item {
        objectName: "root"
        <style>.theme: <style>.System
        property int theme: <style>.theme
        Item { id: light; <style>.theme: <style>.Light }
        Item {
          objectName: "inside"
          property int theme: <style>.theme
          property bool light: <style>.background === light.<style>.background
        }
      }
      """
    Then the property "theme" of "root" is 0
    And the property "theme" of "inside" is 0
    And the property "light" of "inside" is true

    Examples:
      | style     |
      | Material  |
      | Universal |

  @ljos-only
  Scenario Outline: System is Dark in a dark scheme in <style>
    Given the system setting "color scheme" is "dark"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.<style>
      Item {
        objectName: "root"
        <style>.theme: <style>.System
        property int theme: <style>.theme
        Item { id: dark; <style>.theme: <style>.Dark }
        Item {
          objectName: "inside"
          property int theme: <style>.theme
          property bool dark: <style>.background === dark.<style>.background
        }
      }
      """
    Then the property "theme" of "root" is 1
    And the property "theme" of "inside" is 1
    And the property "dark" of "inside" is true

    Examples:
      | style     |
      | Material  |
      | Universal |

  @ljos-only
  Scenario Outline: System follows the scheme when it changes in <style>
    Given the system setting "color scheme" is "light"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.<style>
      Item {
        <style>.theme: <style>.System
        Item { objectName: "inside"; property int theme: <style>.theme }
      }
      """
    When the system setting "color scheme" changes to "dark"
    Then the property "theme" of "inside" is 1

    Examples:
      | style     |
      | Material  |
      | Universal |
