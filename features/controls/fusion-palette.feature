# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#palette-prop
#   https://doc.qt.io/qt-6/qml-palette.html
# Verified on Qt 6.11.2 (qml6, offscreen): every role of a control nobody
# gave a colour, enabled and not; the palette is the imported style's
# whatever style the program started with. The oracle cannot be put in a
# dark colour scheme, so only the light colours are here.

@qt-parity @api-QtQuick.Controls.Control.palette
Feature: The Fusion style's colours
  An application that uses the Fusion style has its palette: a light grey
  window and button, black text, and a blue highlight. Every control has
  it, those the style draws as Basic does too, and a control that is not
  enabled has a paler text and highlight.

  Scenario Outline: A control nobody gave a colour has Fusion's <role>
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
        Frame { objectName: "on" }
        Frame { objectName: "off"; enabled: false }
      }
      """
    Then the property "palette.<role>" of "on" is <enabled>
    And the property "palette.<role>" of "off" is <disabled>

    Examples:
      | role            | enabled     | disabled    |
      | accent          | "#308cc6"   | "#919191"   |
      | alternateBase   | "#f7f7f7"   | "#f7f7f7"   |
      | base            | "#ffffff"   | "#efefef"   |
      | brightText      | "#ffffff"   | "#ffffff"   |
      | button          | "#efefef"   | "#efefef"   |
      | buttonText      | "#000000"   | "#bebebe"   |
      | dark            | "#9f9f9f"   | "#bebebe"   |
      | highlight       | "#308cc6"   | "#919191"   |
      | highlightedText | "#ffffff"   | "#ffffff"   |
      | light           | "#ffffff"   | "#ffffff"   |
      | link            | "#0000ff"   | "#0000ff"   |
      | linkVisited     | "#ff00ff"   | "#ff00ff"   |
      | mid             | "#b8b8b8"   | "#b8b8b8"   |
      | midlight        | "#cacaca"   | "#cacaca"   |
      | placeholderText | "#80000000" | "#80000000" |
      | shadow          | "#767676"   | "#b1b1b1"   |
      | text            | "#000000"   | "#bebebe"   |
      | toolTipBase     | "#ffffdc"   | "#ffffdc"   |
      | toolTipText     | "#000000"   | "#000000"   |
      | window          | "#efefef"   | "#efefef"   |
      | windowText      | "#000000"   | "#bebebe"   |

  Scenario Outline: A <control> has Fusion's colours though the style draws it as Basic does
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        <control> { objectName: "x" }
      }
      """
    Then the property "palette.window" of "x" is "#efefef"
    And the property "palette.windowText" of "x" is "#000000"

    Examples:
      | control  |
      | Label    |
      | Slider   |
      | ComboBox |

  Scenario Outline: The palette is the style the application imports
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=<started>
      """
    And the QML:
      """qml
      import QtQuick
      import <module>
      Item {
        Label { objectName: "x"; text: "a" }
      }
      """
    Then the property "palette.window" of "x" is <window>

    Examples:
      | started | module                  | window    |
      | Basic   | QtQuick.Controls.Fusion | "#efefef" |
      | Fusion  | QtQuick.Controls.Basic  | "#ffffff" |
      | Basic   | QtQuick.Controls        | "#ffffff" |

