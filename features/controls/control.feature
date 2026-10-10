# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Control
  Control is what every control is built on: a background, a content item,
  padding around the content and insets around the background.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Control {
          objectName: "c"
          padding: 10
          background: Rectangle { objectName: "bg"; implicitWidth: 80; implicitHeight: 30 }
          contentItem: Rectangle { objectName: "content"; implicitWidth: 40; implicitHeight: 50 }
        }
      }
      """

  @api-QtQuick.Controls.Control.implicitBackgroundWidth @api-QtQuick.Controls.Control.implicitContentWidth
  Scenario Outline: The implicit size is the larger of the background and the padded content
    Then the property "<property>" of "c" is <value>

    Examples:
      | property                 | value |
      | implicitBackgroundWidth  | 80    |
      | implicitBackgroundHeight | 30    |
      | implicitContentWidth     | 40    |
      | implicitContentHeight    | 50    |
      | implicitWidth            | 80    |
      | implicitHeight           | 70    |
      | width                    | 80    |
      | height                   | 70    |

  @api-QtQuick.Controls.Control.availableWidth
  Scenario Outline: The content item fills what the padding leaves
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object  | property        | value |
      | c       | availableWidth  | 60    |
      | c       | availableHeight | 50    |
      | content | x               | 10    |
      | content | y               | 10    |
      | content | width           | 60    |
      | content | height          | 50    |

  Scenario Outline: The background fills the control and lies behind the content
    Then the property "<property>" of "bg" is <value>

    Examples:
      | property | value |
      | x        | 0     |
      | y        | 0     |
      | width    | 80    |
      | height   | 70    |
      | z        | -1    |

  Scenario: The parts are children of the control
    Then the property "parent.objectName" of "bg" is "c"
    And the property "parent.objectName" of "content" is "c"

  Scenario: The parts follow the size of the control
    When the property "width" of "c" is set to 200
    And the property "height" of "c" is set to 100
    Then the property "width" of "bg" is 200
    And the property "height" of "bg" is 100
    And the property "width" of "content" is 180
    And the property "height" of "content" is 80
    And the property "availableWidth" of "c" is 180

  @api-QtQuick.Controls.Control.padding
  Scenario Outline: Padding of one side wins over the padding of an axis, which wins over the padding
    When the property "horizontalPadding" of "c" is set to 20
    And the property "leftPadding" of "c" is set to 4
    And the property "verticalPadding" of "c" is set to 2
    And the property "bottomPadding" of "c" is set to 7
    Then the property "<property>" of "c" is <value>

    Examples:
      | property       | value |
      | padding        | 10    |
      | leftPadding    | 4     |
      | rightPadding   | 20    |
      | topPadding     | 2     |
      | bottomPadding  | 7     |
      | implicitWidth  | 80    |
      | implicitHeight | 59    |
      | availableWidth | 56    |

  Scenario: The sides follow a later change of the padding
    When the property "padding" of "c" is set to 30
    Then the property "leftPadding" of "c" is 30
    And the property "bottomPadding" of "c" is 30
    And the property "horizontalPadding" of "c" is 30
    And the property "implicitWidth" of "c" is 100
    And the property "implicitHeight" of "c" is 110

  @api-QtQuick.Controls.Control.topInset
  Scenario Outline: Insets move the edges of the background inwards
    When the property "leftInset" of "c" is set to 3
    And the property "topInset" of "c" is set to 5
    And the property "rightInset" of "c" is set to 7
    And the property "bottomInset" of "c" is set to 9
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property      | value |
      | bg     | x             | 3     |
      | bg     | y             | 5     |
      | bg     | width         | 80    |
      | bg     | height        | 56    |
      | c      | implicitWidth | 90    |
      | c      | width         | 90    |

  Scenario: The available size is never negative
    When the property "width" of "c" is set to 5
    Then the property "availableWidth" of "c" is 0
    And the property "width" of "content" is 0

  @api-QtQuick.Controls.Control.contentItem
  Scenario: A content item assigned later takes the place of the first
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Control {
        objectName: "swap"
        padding: 5
        contentItem: Rectangle { objectName: "first"; implicitWidth: 20; implicitHeight: 20 }
        property Item other: Rectangle { objectName: "second"; implicitWidth: 60; implicitHeight: 10 }
        property Item old
        function swap() { old = contentItem; contentItem = other }
      }
      """
    When the method "swap" of "swap" is called
    Then the property "implicitWidth" of "swap" is 70
    And the property "implicitHeight" of "swap" is 20
    And the property "parent.objectName" of "second" is "swap"
    And the property "x" of "second" is 5
    And the property "width" of "second" is 60
    And the property "old.objectName" of "swap" is "first"
    And the property "old.visible" of "swap" is false
    And the property "old.parent" of "swap" is null

  @api-QtQuick.Controls.Control.background
  Scenario: A background with a size of its own keeps it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Control {
        objectName: "own"
        width: 100; height: 100
        background: Rectangle { objectName: "ownBg"; width: 20; height: 30 }
      }
      """
    Then the property "width" of "ownBg" is 20
    And the property "height" of "ownBg" is 30

  Scenario: A control without parts has no size
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Control { objectName: "bare" }
      """
    Then the property "implicitWidth" of "bare" is 0
    And the property "implicitHeight" of "bare" is 0
    And the property "padding" of "bare" is 0
    And the property "spacing" of "bare" is 0
    And the property "hovered" of "bare" is false
    And the property "mirrored" of "bare" is false

  @api-QtQuick.Controls.Control.mirrored
  Scenario: A control is mirrored when its layout is
    When the property "LayoutMirroring.enabled" of "c" is set to true
    Then the property "mirrored" of "c" is true

  Scenario: A disabled control disables its parts
    When the property "enabled" of "c" is set to false
    Then the property "enabled" of "content" is false
    And the property "enabled" of "bg" is false

  @backlog
  Scenario: Assigning undefined to a side makes it follow the padding again
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Control {
        objectName: "r"
        padding: 10
        leftPadding: 4
        function reset() { leftPadding = undefined }
      }
      """
    When the method "reset" of "r" is called
    Then the property "leftPadding" of "r" is 10
