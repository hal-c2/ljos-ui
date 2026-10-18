# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-material.html
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qtquickcontrols-universal.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Material, Fusion and Universal styles
  Each built-in style module provides the full set of controls, and the
  common ones have the style's own default metrics and colours. Material
  and Universal also attach theme and
  palette properties that propagate down the item hierarchy and reset back to
  the inherited or default values.

  Scenario Outline: Material controls have their style metrics and colours
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Item {
        width: 240; height: 200
        Pane { objectName: "pane"; width: 40; height: 40 }
        Button { objectName: "btn"; y: 45 }
        CheckBox { objectName: "chk"; y: 100 }
        Switch { objectName: "sw"; x: 80; y: 100 }
        Slider { objectName: "sl"; y: 150 }
        TextField { objectName: "tf"; x: 100; y: 45 }
      }
      """
    Then the property "<property>" of "<object>" is <value>
    And the pixel at 10, 10 of the window is "#fffbfe"

    Examples:
      | object | property         | value     |
      | pane   | padding          | 12        |
      | pane   | background.color | "#fffbfe" |
      | btn    | implicitWidth    | 64        |
      | btn    | implicitHeight   | 52        |
      | btn    | topPadding       | 14        |
      | btn    | bottomPadding    | 14        |
      | btn    | leftPadding      | 24        |
      | btn    | spacing          | 8         |
      | chk    | implicitHeight   | 48        |
      | chk    | padding          | 8         |
      | chk    | topPadding       | 15        |
      | chk    | bottomPadding    | 15        |
      | chk    | spacing          | 8         |
      | sw     | padding          | 8         |
      | sw     | spacing          | 8         |
      | sl     | implicitWidth    | 200       |
      | sl     | implicitHeight   | 48        |
      | sl     | padding          | 6         |
      | tf     | implicitWidth    | 120       |
      | tf     | implicitHeight   | 56        |
      | tf     | topPadding       | 8         |
      | tf     | bottomPadding    | 8         |
      | tf     | leftPadding      | 16        |
      | tf     | rightPadding     | 16        |

  Scenario: Material attached properties propagate, follow the theme and reset
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Item {
        objectName: "root"
        width: 120; height: 80
        property color red500: Material.color(Material.Red)
        property color red200: Material.color(Material.Red, Material.Shade200)
        property int theme: Material.theme
        property color primary: Material.primary
        property color accent: Material.accent
        property color fg: Material.foreground
        property color bg: Material.background
        Pane {
          objectName: "host"; width: 60; height: 60
          function toDark() { Material.theme = Material.Dark; Material.accent = Material.Red }
          function reset() { Material.accent = undefined; Material.theme = undefined }
          RoundButton {
            objectName: "rb"; checkable: true
            property int theme: Material.theme
            property color accent: Material.accent
            property color fg: Material.foreground
            Material.background: checked ? Material.accent : "transparent"
          }
        }
      }
      """
    Then the property "red500" of "root" is "#f44336"
    And the property "red200" of "root" is "#ef9a9a"
    And the property "theme" of "root" is 0
    And the property "primary" of "root" is "#3f51b5"
    And the property "accent" of "root" is "#e91e63"
    And the property "fg" of "root" is "#dd000000"
    And the property "bg" of "root" is "#fffbfe"
    When the method "toDark" of "host" is called
    And the property "checked" of "rb" is set to true
    Then the property "theme" of "rb" is 1
    And the property "accent" of "rb" is "#ef9a9a"
    And the property "fg" of "rb" is "#ffffff"
    And the property "background.color" of "host" is "#1c1b1f"
    And the property "background.color" of "rb" is "#ef9a9a"
    And the pixel at 55, 55 of the window is "#1c1b1f"
    When the method "reset" of "host" is called
    Then the property "theme" of "rb" is 0
    And the property "accent" of "rb" is "#e91e63"
    And the property "background.color" of "host" is "#fffbfe"

  Scenario Outline: Fusion controls have their style metrics and colours
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Fusion
      Item {
        objectName: "root"
        width: 240; height: 160
        property bool highContrast: Fusion.highContrast
        property color lightShade: Fusion.lightShade
        property color darkShade: Fusion.darkShade
        property color topShadow: Fusion.topShadow
        property color innerContrastLine: Fusion.innerContrastLine
        Button { objectName: "btn" }
        CheckBox { objectName: "chk"; y: 30 }
        Switch { objectName: "sw"; x: 80; y: 30 }
        Slider { objectName: "sl"; y: 70 }
        TextField { objectName: "tf"; y: 95 }
        Pane { objectName: "pane"; y: 130; width: 30; height: 20 }
      }
      """
    Then the property "<property>" of "<object>" is <value>
    And the pixel at 10, 140 of the window is within 10 of "#efefef"

    Examples:
      | object | property       | value       |
      | root   | highContrast   | false       |
      | root   | lightShade     | "#5affffff" |
      | root   | darkShade      | "#3c000000" |
      | root   | topShadow      | "#12000000" |
      | root   | innerContrastLine | "#1effffff" |
      | btn    | implicitWidth  | 80          |
      | btn    | implicitHeight | 24          |
      | btn    | padding        | 4           |
      | btn    | spacing        | 6           |
      | chk    | padding        | 6           |
      | chk    | spacing        | 6           |
      | sw     | padding        | 6           |
      | sw     | spacing        | 6           |
      | sl     | implicitWidth  | 160         |
      | sl     | implicitHeight | 13          |
      | sl     | padding        | 0           |
      | tf     | implicitWidth  | 120         |
      | tf     | padding        | 4           |
      | pane   | padding        | 9           |

  Scenario Outline: Universal controls have their style metrics and colours
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Universal
      Item {
        width: 240; height: 180
        Button { objectName: "btn" }
        CheckBox { objectName: "chk"; y: 40 }
        Switch { objectName: "sw"; x: 80; y: 40 }
        Slider { objectName: "sl"; y: 80 }
        TextField { objectName: "tf"; y: 125 }
        Pane { objectName: "pane"; x: 100; y: 125; width: 40; height: 40 }
      }
      """
    Then the property "<property>" of "<object>" is <value>
    And the pixel at 110, 135 of the window is "#ffffff"

    Examples:
      | object | property         | value     |
      | btn    | implicitWidth    | 32        |
      | btn    | implicitHeight   | 32        |
      | btn    | padding          | 8         |
      | btn    | topPadding       | 4         |
      | btn    | bottomPadding    | 4         |
      | btn    | spacing          | 8         |
      | chk    | implicitHeight   | 32        |
      | chk    | padding          | 6         |
      | chk    | spacing          | 8         |
      | sw     | implicitHeight   | 30        |
      | sw     | padding          | 5         |
      | sw     | spacing          | 8         |
      | sl     | implicitWidth    | 200       |
      | sl     | implicitHeight   | 36        |
      | sl     | padding          | 6         |
      | tf     | implicitWidth    | 60        |
      | tf     | padding          | 12        |
      | tf     | topPadding       | 5         |
      | tf     | bottomPadding    | 7         |
      | tf     | rightPadding     | 8         |
      | pane   | padding          | 12        |
      | pane   | background.color | "#ffffff" |

  Scenario: Universal attached properties propagate, follow the theme and reset
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Universal
      Item {
        objectName: "root"
        width: 100; height: 80
        property color crimson: Universal.color(Universal.Crimson)
        property int theme: Universal.theme
        property color accent: Universal.accent
        property color fg: Universal.foreground
        property color bg: Universal.background
        Pane {
          objectName: "host"; width: 60; height: 60
          function toDark() { Universal.theme = Universal.Dark; Universal.accent = Universal.Crimson }
          function reset() { Universal.accent = undefined; Universal.theme = undefined }
          Item {
            objectName: "child"
            property int theme: Universal.theme
            property color accent: Universal.accent
            property color fg: Universal.foreground
            property color bg: Universal.background
          }
        }
      }
      """
    Then the property "crimson" of "root" is "#a20025"
    And the property "theme" of "root" is 0
    And the property "accent" of "root" is "#3e65ff"
    And the property "fg" of "root" is "#000000"
    And the property "bg" of "root" is "#ffffff"
    When the method "toDark" of "host" is called
    Then the property "theme" of "child" is 1
    And the property "accent" of "child" is "#a20025"
    And the property "fg" of "child" is "#ffffff"
    And the property "bg" of "child" is "#000000"
    And the property "background.color" of "host" is "#000000"
    And the pixel at 20, 20 of the window is "#000000"
    When the method "reset" of "host" is called
    Then the property "theme" of "child" is 0
    And the property "accent" of "child" is "#3e65ff"
    And the property "bg" of "child" is "#ffffff"

  Scenario: Material custom colours are inherited and elevation stays local
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Item {
        objectName: "root"
        Material.primary: "#123456"
        Material.foreground: "#234567"
        Material.background: "#345678"
        Material.elevation: 7
        property int elevation: Material.elevation
        function change() { Material.primary = "#456789"; Material.foreground = "#56789a" }
        function reset() { Material.elevation = undefined }
        Item {
          objectName: "child"
          property color primary: Material.primary
          property color fg: Material.foreground
          property color bg: Material.background
          property int elevation: Material.elevation
        }
      }
      """
    Then the property "primary" of "child" is "#123456"
    And the property "fg" of "child" is "#234567"
    And the property "bg" of "child" is "#345678"
    And the property "elevation" of "root" is 7
    And the property "elevation" of "child" is 0
    When the method "change" of "root" is called
    Then the property "primary" of "child" is "#456789"
    And the property "fg" of "child" is "#56789a"
    When the method "reset" of "root" is called
    Then the property "elevation" of "root" is 0

  Scenario Outline: The colours a style derives follow its theme
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      import QtQuick.Controls.Universal
      Item {
        objectName: "root"
        Material.theme: Material.<theme>
        Universal.theme: Universal.<theme>
        property color frame: Material.frameColor
        property color hint: Material.hintTextColor
        property color dialog: Material.dialogColor
        property color chromeHigh: Universal.chromeHighColor
        property color chromeMedium: Universal.chromeMediumColor
        property color listLow: Universal.listLowColor
      }
      """
    Then the property "<property>" of "root" is <value>

    Examples:
      | theme | property     | value       |
      | Light | frame        | "#60000000" |
      | Dark  | frame        | "#4cffffff" |
      | Light | hint         | "#60000000" |
      | Dark  | hint         | "#4cffffff" |
      | Light | dialog       | "#ffffff"   |
      | Dark  | dialog       | "#424242"   |
      | Light | chromeHigh   | "#cccccc"   |
      | Dark  | chromeHigh   | "#767676"   |
      | Light | chromeMedium | "#e6e6e6"   |
      | Dark  | chromeMedium | "#1f1f1f"   |
      | Light | listLow      | "#19000000" |
      | Dark  | listLow      | "#19ffffff" |

  # Ljós's styles restyle some controls and give the rest Basic's look;
  # whichever look a control has, every control is in every style module.
  Scenario Outline: Every control is in every style module
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.<style>
      Item {
        SpinBox { objectName: "spin"; from: 0; to: 10; value: 4 }
        ComboBox { objectName: "combo"; model: ["a", "b", "c"] }
      }
      """
    When the method "increase" of "spin" is called
    Then the property "value" of "spin" is 5
    And the property "count" of "combo" is 3

    Examples:
      | style     |
      | Material  |
      | Fusion    |
      | Universal |
