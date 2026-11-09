# Sources:
#   docs/DIALECT.md (§4.16 the controls)
#   https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#mirrored-prop
# Ljós's own style is Ljós's: Qt has no Ljos.Controls to compare it with, so
# every scenario is `@ljos-only`.

@ljos-only
Feature: Ljós's own style, mirrored
  A control that is mirrored, by LayoutMirroring or by a right-to-left
  locale, lays its parts out from the right: what is at its start (an
  icon, a label, the column a row keeps for icons) is at its right, and
  what is at its end (a chevron, a shortcut, the room kept for a check)
  at its left. Mirroring moves anchors and nothing else, so each part a
  control of Ljos.Controls places by `x` is placed for both directions,
  and its padding is kept on the side its part is on.

  Rule: A combo box keeps its chevron's room and its icon on the sides they are drawn on

    Scenario Outline: A combo box <how> has the <property> of its <part> at <value>
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Item {
          width: 320; height: 120
          ComboBox {
            objectName: "c"; x: 10; y: 10; width: 160
            model: ["Small", "Large"]; iconName: "plus"
            LayoutMirroring.enabled: <mirrored>
          }
        }
        """
      Then the property "<property>" of "<part>" is <value>

      Examples:
        | how          | mirrored | part  | property            | value |
        | mirrored     | true     | c     | leftPadding         | 24    |
        | mirrored     | true     | c     | rightPadding        | 32    |
        | mirrored     | true     | c     | indicator.x         | 8     |
        | mirrored     | true     | glyph | x                   | 112   |
        | mirrored     | true     | label | horizontalAlignment | 2     |
        | not mirrored | false    | c     | leftPadding         | 32    |
        | not mirrored | false    | c     | rightPadding        | 24    |
        | not mirrored | false    | c     | indicator.x         | 140   |
        | not mirrored | false    | glyph | x                   | -24   |
        | not mirrored | false    | label | horizontalAlignment | 1     |

    Scenario: The rows of a mirrored combo box have their check at the left
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Item {
          width: 320; height: 240
          ComboBox {
            objectName: "c"; x: 10; y: 10; width: 160
            model: ["Small"]
            LayoutMirroring.enabled: true
            Component.onCompleted: popup.open()
          }
        }
        """
      Then the property "x" of "check" is 0
      And the property "visible" of "check" is true
      And the property "x" of "rowLabel" is 24
      And the property "horizontalAlignment" of "rowLabel" is 2

  Rule: A menu's row lays out from the right

    Scenario: A mirrored row has its icon at the right and its shortcut past the check at the left
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Item {
          width: 320; height: 240
          Menu {
            Component.onCompleted: open()
            MenuItem {
              objectName: "wrap"; text: "Wrap"; iconName: "minus"; shortcut: "Ctrl+W"
              checkable: true; checked: true
              LayoutMirroring.enabled: true
            }
          }
        }
        """
      Then the property "indicator.x" of "wrap" is 8
      And the property "contentItem.width" of "wrap" is 196
      And the property "x" of "glyph" is 180
      And the property "x" of "keys" is 24
      And the property "x" of "label" is at least 60
      # At its start (Qt.AlignLeft | Qt.AlignVCenter): a mirrored label
      # sets its start at the right.
      And the property "mirrored" of "label" is true
      And the property "alignment" of "label" is 129

    Scenario: A row that is not mirrored has its icon at the left and its shortcut before the check
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Item {
          width: 320; height: 240
          Menu {
            Component.onCompleted: open()
            MenuItem {
              objectName: "wrap"; text: "Wrap"; iconName: "minus"; shortcut: "Ctrl+W"
              checkable: true; checked: true
            }
          }
        }
        """
      Then the property "indicator.x" of "wrap" is 188
      And the property "x" of "glyph" is 0
      And the property "x" of "label" is 24
      # At its start (Qt.AlignLeft | Qt.AlignVCenter), the left.
      And the property "mirrored" of "label" is false
      And the property "alignment" of "label" is 129

    # A menu's delegate is used for its submenus' rows too, so a checked
    # delegate has a check and a chevron in the same row.
    Scenario Outline: A checked submenu row <how> has its chevron past the check's room
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Item {
          width: 320; height: 240
          Menu {
            Component.onCompleted: open()
            delegate: MenuItem {
              objectName: "row"; checkable: true; checked: true
              LayoutMirroring.enabled: <mirrored>
            }
            Menu { title: "Recent" }
          }
        }
        """
      Then the property "subMenu" of "row" is not null
      And the property "contentItem.width" of "row" is 196
      And the property "indicator.x" of "row" is <check>
      And the property "x" of "more" is <chevron>

      Examples:
        | how          | mirrored | check | chevron |
        | mirrored     | true     | 8     | 24      |
        | not mirrored | false    | 188   | 160     |

  Rule: A button and a labelled control lay out from the right

    Scenario: A mirrored button has its icon at the right and its chevron at the left
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Item {
          width: 320; height: 120
          Button {
            objectName: "b"; x: 10; y: 10; width: 120
            text: "Add"; iconName: "plus"; chevron: true
            LayoutMirroring.enabled: true
          }
        }
        """
      Then the property "contentItem.width" of "b" is 104
      And the property "x" of "glyph" is 88
      And the property "x" of "more" is 0
      And the property "x" of "label" is 20
      And the property "width" of "label" is 60
      # At its start (Qt.AlignLeft | Qt.AlignVCenter): a mirrored label
      # sets its start at the right.
      And the property "mirrored" of "label" is true
      And the property "alignment" of "label" is 129

    Scenario Outline: A mirrored <control>'s label is set against its indicator at the right
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Item {
          width: 320; height: 120
          <control> { objectName: "c"; x: 10; y: 10; width: 200; text: "Wrap lines"; LayoutMirroring.enabled: true }
        }
        """
      Then the property "horizontalAlignment" of "label" is 2

      Examples:
        | control  |
        | CheckBox |
        | Switch   |

  Rule: Mirrored, every control still passes the design audit

    Scenario: A mirrored window of the controls has nothing to report
      Given the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Rectangle {
          width: 480; height: 400; color: Theme.canvas
          LayoutMirroring.enabled: true
          LayoutMirroring.childrenInherit: true
          Grid {
            x: 16; y: 16; columns: 2; spacing: 12
            Button { text: "Add"; iconName: "plus"; chevron: true }
            Button { text: "Send"; primary: true }
            Switch { text: "Wrap lines" }
            CheckBox { text: "Show hidden files"; checked: true }
            ComboBox { model: ["Small", "Medium", "Large"]; iconName: "plus"; Accessible.name: "Size" }
            ComboBox { model: ["One", "Two"]; outline: true; Accessible.name: "Count" }
            TextField { placeholderText: "Name"; Accessible.name: "Name" }
            SpinBox { value: 4; Accessible.name: "Columns" }
            Slider { value: 0.5; Accessible.name: "Volume" }
            Label { text: "Ready" }
          }
        }
        """
      Then the design audit finds nothing
