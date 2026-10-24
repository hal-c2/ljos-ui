# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-palette.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.Fusion
Feature: Fusion's shades of a palette
  Fusion shades its buttons, grooves and tab frames from the palette's
  button colour, and outlines them from its window colour, so a palette of
  one's own gives the whole style new shades. The `Fusion` singleton gives
  the same shades to a style of one's own.

  Scenario Outline: Fusion's <shade> of a grey button colour
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Fusion
      Item {
        Button { id: b; palette.button: "#808080"; palette.window: "#808080" }
        QtObject {
          objectName: "shades"
          property color plain: Fusion.buttonColor(b.palette, false, false, false)
          property color down: Fusion.buttonColor(b.palette, false, true, false)
          property color hovered: Fusion.buttonColor(b.palette, false, false, true)
          property color start: Fusion.gradientStart("#808080")
          property color stop: Fusion.gradientStop("#808080")
          property color groove: Fusion.grooveColor(b.palette)
          property color tabFrame: Fusion.tabFrameColor(b.palette)
          property color outline: Fusion.outline(b.palette)
          property color offOutline: Fusion.buttonOutline(b.palette, false, false)
          property color merged: Fusion.mergedColors("#ff0000", "#0000ff", 25)
        }
      }
      """
    Then the property "<shade>" of "shades" is <value>

    Examples:
      | shade      | value     |
      | plain      | "#858585" |
      | down       | "#797979" |
      | hovered    | "#8a8a8a" |
      | start      | "#9f9f9f" |
      | stop       | "#838383" |
      | groove     | "#777777" |
      | tabFrame   | "#8a8a8a" |
      | outline    | "#5b5b5b" |
      | offOutline | "#696969" |
      | merged     | "#3f00bf" |

  Scenario Outline: Fusion's highlighted <shade> of <button> under <highlight>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Fusion
      Item {
        Button { id: b; palette.button: "<button>"; palette.highlight: "<highlight>" }
        QtObject {
          objectName: "shades"
          property color plain: Fusion.buttonColor(b.palette, true, false, false)
          property color down: Fusion.buttonColor(b.palette, true, true, false)
          property color hovered: Fusion.buttonColor(b.palette, true, false, true)
          property color both: Fusion.buttonColor(b.palette, true, true, true)
          property color outline: Fusion.highlightedOutline(b.palette)
        }
      }
      """
    Then the property "<shade>" of "shades" is <value>

    Examples: A dark highlight, whose outline is the highlight darkened
      | button  | highlight | shade   | value     |
      | #808080 | #308cc6   | plain   | "#7c858a" |
      | #808080 | #308cc6   | down    | "#71797e" |
      | #808080 | #308cc6   | hovered | "#818a90" |
      | #808080 | #308cc6   | both    | "#757d83" |
      | #808080 | #308cc6   | outline | "#26709e" |

    Examples: A light highlight, whose outline keeps to a lightness of 160
      | button  | highlight | shade   | value     |
      | #8cf881 | #fa70a4   | plain   | "#abe79d" |
      | #8cf881 | #fa70a4   | down    | "#9cd28e" |
      | #8cf881 | #fa70a4   | hovered | "#b2f0a3" |
      | #8cf881 | #fa70a4   | both    | "#a2da94" |
      | #8cf881 | #fa70a4   | outline | "#d56b94" |
      | #808080 | #ffffff   | plain   | "#8a8a8a" |
      | #808080 | #ffffff   | outline | "#a0a0a0" |

  Scenario Outline: A control's <property> with a grey button colour
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Column {
        palette.button: "#808080"
        Button { objectName: "button"; text: "Ab" }
        ToolButton { objectName: "down"; text: "Ab"; down: true }
        TabButton { objectName: "tab"; text: "Ab" }
        TabButton { objectName: "current"; text: "Ab"; checked: true }
        Slider { objectName: "slider"; value: 0.5 }
      }
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object  | property                          | value     |
      | button  | background.color                  | "#858585" |
      | button  | background.gradient.stops.0.color | "#a5a5a5" |
      | button  | background.gradient.stops.1.color | "#878787" |
      | button  | background.border.color           | "#ababab" |
      | down    | background.color                  | "#797979" |
      | tab     | background.gradient.stops.0.color | "#808080" |
      | tab     | background.gradient.stops.2.color | "#777777" |
      | current | background.gradient.stops.0.color | "#909090" |
      | current | background.gradient.stops.2.color | "#8a8a8a" |
      | slider  | background.gradient.stops.0.color | "#6c6c6c" |
      | slider  | background.gradient.stops.1.color | "#838383" |
      | slider  | background.border.color           | "#ababab" |

  Scenario: Fusion's outlines come from the window colour
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Column {
        palette.window: "#808080"
        Button { objectName: "button"; text: "Ab" }
        Button { objectName: "off"; text: "Ab"; enabled: false }
      }
      """
    Then the property "background.border.color" of "button" is "#5b5b5b"
    And the property "background.border.color" of "off" is "#696969"
    And the property "background.color" of "button" is "#e8e8e8"

  Scenario: The shades follow a palette that changes
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Column {
        Button { objectName: "button"; text: "Ab" }
        Slider { objectName: "slider"; value: 0.5 }
      }
      """
    When the property "palette.button" of "button" is set to "#808080"
    And the property "palette.button" of "slider" is set to "#808080"
    And the property "palette.window" of "button" is set to "#808080"
    Then the property "background.color" of "button" is "#858585"
    And the property "background.gradient.stops.0.color" of "button" is "#a5a5a5"
    And the property "background.border.color" of "button" is "#5b5b5b"
    And the property "background.gradient.stops.0.color" of "slider" is "#6c6c6c"
