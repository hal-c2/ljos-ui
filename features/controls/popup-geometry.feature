# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#popup-sizing
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#popup-positioning
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#margins-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#anchors.centerIn-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Popup sizing and positioning
  A popup lays its background and its content item out like a control
  does. It is placed from its parent's corner or centred in an item, and
  its margins keep it inside the window.

  @api-QtQuick.Controls.Popup.padding @api-QtQuick.Controls.Popup.topInset
  Scenario Outline: Padding and insets place the content item and the background
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Popup {
          objectName: "p"
          width: 100; height: 80
          padding: 5; topPadding: 20
          leftInset: 3; topInset: 4
          background: Rectangle { implicitWidth: 200; implicitHeight: 150 }
          visible: true
        }
      }
      """
    Then the property "<property>" of "p" is <value>

    Examples:
      | property                 | value |
      | implicitWidth            | 203   |
      | implicitHeight           | 154   |
      | implicitBackgroundWidth  | 200   |
      | implicitBackgroundHeight | 150   |
      | leftPadding              | 5     |
      | bottomPadding            | 5     |
      | topPadding               | 20    |
      | contentItem.x            | 5     |
      | contentItem.y            | 20    |
      | contentItem.width        | 90    |
      | contentItem.height       | 55    |
      | availableWidth           | 90    |
      | availableHeight          | 55    |
      | background.x             | 3     |
      | background.y             | 4     |
      | background.width         | 97    |
      | background.height        | 76    |

  @api-QtQuick.Controls.Popup.horizontalPadding
  Scenario Outline: The padding of a side follows its axis, and that the padding
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Popup {
          objectName: "p"
          padding: 4; horizontalPadding: 10; bottomPadding: 1
        }
      }
      """
    Then the property "<property>" of "p" is <value>

    Examples:
      | property        | value |
      | leftPadding     | 10    |
      | rightPadding    | 10    |
      | topPadding      | 4     |
      | bottomPadding   | 1     |
      | verticalPadding | 4     |
      | implicitWidth   | 20    |
      | implicitHeight  | 5     |

  Scenario Outline: A popup is at least as large as its background wants to be
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Popup {
          objectName: "p"
          background: Rectangle { implicitWidth: 200; implicitHeight: 150 }
          Item { implicitWidth: 10; implicitHeight: 10 }
        }
      }
      """
    Then the property "<property>" of "p" is <value>

    Examples:
      | property           | value |
      | width              | 200   |
      | height             | 150   |
      | contentItem.width  | 176   |
      | contentItem.height | 126   |
      | contentWidth       | 10    |

  Scenario Outline: Children that only have a size do not make the popup larger
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Popup {
          objectName: "p"
          Rectangle { width: 50; height: 30 }
          Rectangle { width: 50; height: 30 }
        }
      }
      """
    Then the property "<property>" of "p" is <value>

    Examples:
      | property               | value |
      | width                  | 24    |
      | height                 | 24    |
      | contentWidth           | 0     |
      | contentChildren.length | 2     |

  @api-QtQuick.Controls.Popup.contentWidth
  Scenario: A content size that is given wins over the children
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Popup {
          objectName: "p"
          contentWidth: 80; contentHeight: 40
          Rectangle { implicitWidth: 50; implicitHeight: 30 }
        }
      }
      """
    Then the property "width" of "p" is 104
    And the property "height" of "p" is 64

  @api-QtQuick.Controls.Popup.contentItem
  Scenario: A content item of the author's is sized by the popup
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 400; height: 400
        Popup {
          objectName: "p"
          padding: 10
          contentItem: Rectangle { objectName: "mine"; implicitWidth: 60; implicitHeight: 20 }
        }
      }
      """
    Then the property "width" of "p" is 80
    And the property "height" of "p" is 40
    And the property "contentItem.x" of "p" is 10
    And the property "contentItem.width" of "p" is 60
    And the property "contentItem.objectName" of "p" is "mine"

  Rule: Margins keep a popup inside the window

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          Item {
            objectName: "holder"
            x: 300; y: 350; width: 50; height: 20
            Popup { objectName: "free"; width: 150; height: 100; x: 10; y: 10 }
            Popup { objectName: "kept"; width: 150; height: 100; x: 10; y: 10; margins: 0 }
            Popup { objectName: "edge"; width: 150; height: 100; x: 10; y: 10; margins: 20 }
            Popup { objectName: "side"; width: 150; height: 100; x: 10; y: 10; rightMargin: 0 }
            Popup { objectName: "left"; width: 50; height: 50; x: -350; y: -380; margins: 5 }
            Popup { objectName: "huge"; width: 500; height: 450; margins: 0 }
          }
        }
        """

    Scenario: Without margins a popup may leave the window
      When the method "open" of "free" is called
      Then the property "contentItem.parent.x" of "free" is 310
      And the property "contentItem.parent.y" of "free" is 360
      And the property "x" of "free" is 10

    @api-QtQuick.Controls.Popup.margins
    Scenario: With margins a popup is moved back into the window
      When the method "open" of "kept" is called
      Then the property "contentItem.parent.x" of "kept" is 250
      And the property "contentItem.parent.y" of "kept" is 300

    Scenario: A popup that was moved says where it is
      When the method "open" of "kept" is called
      Then the property "x" of "kept" is -50
      And the property "y" of "kept" is -50

    Scenario: The margins are kept free at the edges of the window
      When the method "open" of "edge" is called
      Then the property "contentItem.parent.x" of "edge" is 230
      And the property "contentItem.parent.y" of "edge" is 280

    @api-QtQuick.Controls.Popup.rightMargin
    Scenario: A margin on one side keeps the popup in on that side only
      When the method "open" of "side" is called
      Then the property "contentItem.parent.x" of "side" is 250
      And the property "contentItem.parent.y" of "side" is 360

    Scenario: The left and the top margin keep a popup in too
      When the method "open" of "left" is called
      Then the property "contentItem.parent.x" of "left" is 5
      And the property "contentItem.parent.y" of "left" is 5

    Scenario: A popup larger than the window starts at its corner
      When the method "open" of "huge" is called
      Then the property "contentItem.parent.x" of "huge" is 0
      And the property "contentItem.parent.y" of "huge" is 0
      And the property "width" of "huge" is 500

    Scenario Outline: The margin of a side follows the margins
      Then the property "<property>" of "<popup>" is <value>

      Examples:
        | popup | property     | value |
        | edge  | leftMargin   | 20    |
        | edge  | bottomMargin | 20    |
        | side  | rightMargin  | 0     |
        | side  | leftMargin   | -1    |
        | side  | margins      | -1    |

  Rule: A popup that does not fit between its margins is made smaller

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          component Tall: Popup {
            property real rows: 800
            padding: 0
            contentItem: Item { implicitWidth: 100; implicitHeight: rows }
          }
          Tall { objectName: "tall"; margins: 0 }
          Tall { objectName: "free" }
          Tall { objectName: "gap"; y: 100; topMargin: 6; bottomMargin: 6 }
          Tall { objectName: "top"; topMargin: 0 }
          Tall { objectName: "bottom"; bottomMargin: 0 }
          Popup { objectName: "given"; margins: 0; height: 800 }
          Popup {
            objectName: "wide"; y: 100; margins: 10; padding: 0
            contentItem: Item { implicitWidth: 800; implicitHeight: 100 }
          }
        }
        """

    Scenario Outline: The open popup "<popup>" is <height> tall at <y>
      When the method "open" of "<popup>" is called
      Then the property "height" of "<popup>" is <height>
      And the property "y" of "<popup>" is <y>

      Examples:
        | popup  | height | y    |
        | tall   | 400    | 0    |
        | gap    | 388    | 6    |
        | free   | 800    | 0    |
        | top    | 800    | 0    |
        | bottom | 800    | -400 |
        | given  | 800    | 0    |
        | wide   | 100    | 100  |

    Scenario: A popup that is too wide is made narrower
      When the method "open" of "wide" is called
      Then the property "width" of "wide" is 380
      And the property "x" of "wide" is 10

    Scenario: The content item is as tall as the popup has room for
      When the method "open" of "tall" is called
      Then the property "contentItem.height" of "tall" is 400
      And the property "implicitHeight" of "tall" is 800

    Scenario: A popup that was not opened is as tall as it wants to be
      Then the property "height" of "tall" is 800

    Scenario: A popup that fits again is as tall as it wants to be
      When the method "open" of "tall" is called
      And the property "rows" of "tall" is set to 200
      Then the property "height" of "tall" is 200

  Rule: A popup can be centred

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          Item {
            objectName: "holder"
            x: 100; y: 200; width: 200; height: 100
            Popup {
              objectName: "inParent"
              width: 100; height: 40
              anchors.centerIn: parent
            }
            Popup {
              objectName: "inWindow"
              width: 100; height: 40
              anchors.centerIn: Overlay.overlay
            }
          }
        }
        """

    @api-QtQuick.Controls.Popup.anchors.centerIn
    Scenario: A popup centred in its parent
      When the method "open" of "inParent" is called
      Then the property "contentItem.parent.x" of "inParent" is 150
      And the property "contentItem.parent.y" of "inParent" is 230
      And the property "x" of "inParent" is 50
      And the property "y" of "inParent" is 30

    Scenario: A popup centred in the window
      When the method "open" of "inWindow" is called
      Then the property "contentItem.parent.x" of "inWindow" is 150
      And the property "contentItem.parent.y" of "inWindow" is 180

    Scenario: A centred popup stays centred when its size changes
      When the method "open" of "inParent" is called
      And the property "width" of "inParent" is set to 60
      Then the property "contentItem.parent.x" of "inParent" is 170

  Rule: A popup placed by bindings on x and y follows them

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        import QtQuick.Layouts
        Item {
          width: 400; height: 400
          Popup {
            objectName: "p"
            x: Math.round((parent.width - width) / 2)
            y: Math.round((parent.height - height) / 2)
            padding: 0
            contentItem: Rectangle { implicitWidth: 100; implicitHeight: 40 }
          }
          Dialog {
            objectName: "d"
            x: Math.round((parent.width - width) / 2)
            y: Math.round((parent.height - height) / 2)
            title: "Add"
            standardButtons: DialogButtonBox.Ok | DialogButtonBox.Cancel
            contentItem: RowLayout { Tumbler { model: 24 } Tumbler { model: 60 } }
          }
        }
        """

    Scenario: An open popup that grows stays where its bindings put it
      When the method "open" of "p" is called
      And the property "contentItem.implicitWidth" of "p" is set to 200
      And the property "contentItem.implicitHeight" of "p" is set to 100
      Then the property "width" of "p" is 200
      And the property "x" of "p" is 100
      And the property "y" of "p" is 150
      And the property "contentItem.parent.x" of "p" is 100

    Scenario: A dialog that grows as it opens is where its bindings put it
      When the method "open" of "d" is called
      And 1 second passes
      Then the property "width" of "d" is 225
      And the property "x" of "d" is 88
      And the property "y" of "d" is 37

  Rule: A popup in a popup

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          Popup {
            objectName: "outer"
            x: 50; y: 60; width: 200; height: 150
            Popup { objectName: "inner"; x: 10; y: 20; width: 80; height: 40 }
          }
        }
        """

    Scenario: A popup declared in a popup is placed from that popup's content
      When the method "open" of "outer" is called
      And the method "open" of "inner" is called
      Then the property "contentItem.parent.x" of "inner" is 72
      And the property "contentItem.parent.y" of "inner" is 92

    Scenario: The parent of a popup declared in a popup is its content item
      Then the property "parent.width" of "inner" is 176
      And the property "parent.x" of "inner" is 12

  Rule: A text in a popup wraps to the width the popup gives it

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 300
          Popup {
            objectName: "p"
            padding: 20
            anchors.centerIn: parent
            width: parent.width / 2
            contentItem: Text {
              objectName: "t"
              wrapMode: Text.WordWrap
              font.pointSize: 15
              text: "This device cannot grant the right to write system settings."
            }
          }
        }
        """

    @api-QtQuick.Controls.Popup.contentItem
    Scenario: A wrapped text is as wide as the room inside the padding
      Then the property "contentItem.width" of "p" is 160
      And the property "contentItem.lineCount" of "p" is 4
