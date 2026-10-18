# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-fusion.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-groupbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-pageindicator.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-rangeslider.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-delaybutton.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dial.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tumbler.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-checkdelegate.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-radiodelegate.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-switchdelegate.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Fusion's group boxes, indicators, range sliders, dials and delegates
  The Fusion style draws the rest of its controls with the same parts and
  the same palette as its buttons: a group box is a faint frame under its
  title, a range slider is a slider's groove filled between two of its
  handles, a delay button is a button panel that fills in the highlight,
  and a delegate that checks has a check box's, a radio button's or a
  switch's indicator at the far end of its row.

  Background:
    Given the application starts with the controls configuration:
      """
      [Controls]
      Style=Fusion
      """

  @api-QtQuick.Controls.GroupBox
  Scenario Outline: A group box <with>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        GroupBox { objectName: "box"; <title> Label { text: "Inside" } }
      }
      """
    Then the property "leftPadding" of "box" is 9
    And the property "topPadding" of "box" is <top>
    And the property "implicitHeight" of "box" is <height>
    And the property "label.x" of "box" is 9
    And the property "label.y" of "box" is 0
    And the property "background.y" of "box" is <frame>
    And the property "background.radius" of "box" is 2
    And the property "background.color" of "box" is "#03000000"
    And the property "background.border.color" of "box" is "#b8b8b8"

    Examples:
      | with             | title           | top | height | frame |
      | with a title     | title: "Group"; | 29  | 52     | 20    |
      | without a title  |                 | 9   | 32     | 0     |

  @api-QtQuick.Controls.PageIndicator
  Scenario Outline: A page indicator <state>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        PageIndicator { objectName: "dots"; count: 4; currentIndex: 1; enabled: <enabled> }
      }
      """
    Then the property "implicitWidth" of "dots" is 44
    And the property "implicitHeight" of "dots" is 14
    And the property "contentItem.x" of "dots" is 4
    And the property "contentItem.children.0.width" of "dots" is 6
    And the property "contentItem.children.0.radius" of "dots" is 3
    And the property "contentItem.children.0.color" of "dots" is "<color>"
    And the property "contentItem.children.0.opacity" of "dots" is within 0.001 of 0.45
    And the property "contentItem.children.1.opacity" of "dots" is within 0.001 of 0.95

    Examples:
      | state           | enabled | color   |
      | enabled         | true    | #767676 |
      | not enabled     | false   | #b1b1b1 |

  @api-QtQuick.Controls.RangeSlider
  Scenario Outline: A <how> range slider
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        RangeSlider { objectName: "range"; <how>; first.value: 0.25; second.value: 0.75 }
      }
      """
    Then the property "implicitWidth" of "range" is <width>
    And the property "implicitHeight" of "range" is <height>
    And the property "first.handle.x" of "range" is <fx>
    And the property "first.handle.y" of "range" is <fy>
    And the property "second.handle.x" of "range" is <sx>
    And the property "second.handle.y" of "range" is <sy>
    And the property "background.x" of "range" is <gx>
    And the property "background.y" of "range" is <gy>
    And the property "background.children.0.x" of "range" is <fillx>
    And the property "background.children.0.y" of "range" is <filly>
    And the property "background.children.0.border.color" of "range" is "<edge>"

    Examples:
      | how                         | width | height | fx  | fy  | sx  | sy | gx | gy | fillx | filly | edge    |
      | orientation: Qt.Horizontal  | 160   | 13     | 37  | 0   | 110 | 0  | 0  | 4  | 40    | 0     | #236690 |
      | orientation: Qt.Vertical    | 13    | 160    | 0   | 110 | 0   | 37 | 4  | 0  | 0     | 40    | #236690 |
      | enabled: false              | 160   | 13     | 37  | 0   | 110 | 0  | 0  | 4  | 40    | 0     | #696969 |

  @api-QtQuick.Controls.DelayButton
  Scenario Outline: A delay button <state>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        DelayButton { objectName: "delay"; text: "Delay"; <state> }
      }
      """
    Then the property "implicitWidth" of "delay" is 80
    And the property "implicitHeight" of "delay" is 26
    And the property "background.color" of "delay" is "<panel>"
    And the property "background.children.1.width" of "delay" is <fill>
    And the property "background.children.1.border.color" of "delay" is "#22648d"

    Examples:
      | state                       | panel   | fill |
      | progress: 0                 | #e8e8e8 | 0    |
      | progress: 0.5               | #e8e8e8 | 40   |
      | checked: true; progress: 1  | #d3d3d3 | 80   |

  @api-QtQuick.Controls.Dial @api-QtQuick.Controls.Tumbler
  Scenario: A dial's knob and a tumbler's rows
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        Dial { objectName: "dial"; value: 0.3 }
        Tumbler { objectName: "tumbler"; x: 120; model: 5 }
      }
      """
    Then the property "implicitWidth" of "dial" is 100
    And the property "implicitHeight" of "dial" is 100
    And the property "handle.width" of "dial" is within 0.001 of 14.286
    And the property "implicitWidth" of "tumbler" is 60
    And the property "implicitHeight" of "tumbler" is 200
    And the property "currentItem.height" of "tumbler" is 40
    And the property "currentItem.opacity" of "tumbler" is 1

  @api-QtQuick.Controls.ItemDelegate @api-QtQuick.Controls.CheckDelegate
  @api-QtQuick.Controls.RadioDelegate @api-QtQuick.Controls.SwitchDelegate
  Scenario Outline: A <delegate> <state>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Item {
        <delegate> { objectName: "row"; text: "Row"; checked: true; <state> }
      }
      """
    Then the property "implicitWidth" of "row" is 100
    And the property "implicitHeight" of "row" is <height>
    And the property "background.color" of "row" is "<back>"
    And the property "contentItem.color" of "row" is "<ink>"

    Examples:
      | delegate       | state              | height | back    | ink     |
      | ItemDelegate   | down: true         | 26     | #dbdbdb | #000000 |
      | ItemDelegate   | enabled: false     | 26     | #efefef | #bebebe |
      | CheckDelegate  | highlighted: true  | 26     | #308cc6 | #ffffff |
      | CheckDelegate  | enabled: false     | 26     | #efefef | #bebebe |
      | RadioDelegate  | down: true         | 26     | #dbdbdb | #000000 |
      | RadioDelegate  | enabled: false     | 26     | #efefef | #bebebe |
      | SwitchDelegate | highlighted: true  | 28     | #308cc6 | #ffffff |
      | SwitchDelegate | enabled: false     | 28     | #efefef | #bebebe |

  @api-QtQuick.Controls.CheckDelegate @api-QtQuick.Controls.RadioDelegate
  @api-QtQuick.Controls.SwitchDelegate
  Scenario Outline: A <delegate>'s indicator is at the far end of its row
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Column {
        <delegate> { objectName: "row"; text: "Row"; width: 200; enabled: false }
        <delegate> { objectName: "mirrored"; text: "Row"; width: 200; LayoutMirroring.enabled: true }
      }
      """
    Then the property "indicator.x" of "row" is <x>
    And the property "indicator.y" of "row" is 6
    And the property "indicator.width" of "row" is <width>
    And the property "indicator.color" of "row" is "#ffffff"
    And the property "indicator.x" of "mirrored" is 6

    Examples:
      | delegate       | x   | width |
      | CheckDelegate  | 180 | 14    |
      | RadioDelegate  | 180 | 14    |
      | SwitchDelegate | 154 | 40    |
