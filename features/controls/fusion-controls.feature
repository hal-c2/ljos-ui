# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-progressbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-busyindicator.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-radiobutton.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-frame.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.ProgressBar @api-QtQuick.Controls.BusyIndicator
@api-QtQuick.Controls.RadioButton @api-QtQuick.Controls.Frame
Feature: The Fusion style's progress, choices and frames
  Fusion's progress bar is a rounded field as tall as a button with the
  done share in it, framed darker than the highlight. Its busy indicator,
  radio buttons and frames are smaller and closer in than Basic's.

  Background:
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """
    And the file "Controls.qml":
      """qml
      import QtQuick
      import QtQuick.Controls
      Column {
        ProgressBar { objectName: "half"; value: 0.5; width: 120 }
        ProgressBar { objectName: "offBar"; value: 0.5; enabled: false }
        BusyIndicator { objectName: "busy" }
        RadioButton { objectName: "radio"; text: "Ab" }
        RadioButton { objectName: "chosen"; text: "Ab"; checked: true }
        RadioButton { objectName: "pressed"; text: "Ab"; down: true }
        RadioButton { objectName: "offRadio"; text: "Ab"; enabled: false }
        Frame { objectName: "frame" }
      }
      """

  Scenario Outline: The <property> of the <object> control
    Given the QML:
      """qml
      import QtQuick
      Controls {}
      """
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object   | property                            | value       |
      | half     | implicitWidth                       | 120         |
      | half     | implicitHeight                      | 24          |
      | half     | background.color                    | "#ffffff"   |
      | half     | background.border.color             | "#ababab"   |
      | half     | background.radius                   | 2           |
      | half     | contentItem.children.0.width        | 60          |
      | half     | contentItem.children.0.height       | 24          |
      | half     | contentItem.children.0.radius       | 2           |
      | half     | contentItem.children.0.border.color | "#22648d"   |
      | offBar   | background.color                    | "#efefef"   |
      | busy     | implicitWidth                       | 40          |
      | busy     | padding                             | 6           |
      | busy     | contentItem.implicitWidth           | 28          |
      | radio    | implicitHeight                      | 26          |
      | radio    | implicitWidth                       | 46.671875   |
      | radio    | indicator.implicitWidth             | 14          |
      | radio    | indicator.radius                    | 7           |
      | radio    | indicator.color                     | "#ffffff"   |
      | radio    | indicator.border.color              | "#9f9f9f"   |
      | radio    | indicator.children.1.visible        | false       |
      | chosen   | indicator.children.1.visible        | true        |
      | chosen   | indicator.children.1.color          | "#b4000000" |
      | chosen   | indicator.children.1.border.color   | "#c8000000" |
      | pressed  | indicator.color                     | "#d8d8d8"   |
      | offRadio | indicator.color                     | "#ffffff"   |
      | offRadio | contentItem.color                   | "#bebebe"   |
      | frame    | padding                             | 9           |
      | frame    | background.border.color             | "#b8b8b8"   |
