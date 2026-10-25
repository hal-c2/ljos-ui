# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-overlay.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#modal-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#dim-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#z-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Overlay
  The overlay is a layer over the whole window. Open popups are shown in
  it, the last one opened on top, and a popup that dims gets a shade
  between itself and what is under it.

  Rule: The overlay covers the window

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          width: 400; height: 400
          property var ov: Overlay.overlay
          Popup { objectName: "p"; width: 100; height: 100 }
        }
        """

    @api-QtQuick.Controls.Overlay.overlay
    Scenario Outline: The overlay is as large as the window and above its content
      Then the property "<property>" of "root" is <value>

      Examples:
        | property   | value   |
        | ov.width   | 400     |
        | ov.height  | 400     |
        | ov.x       | 0       |
        | ov.z       | 1000001 |
        | ov.visible | false   |

    Scenario: The overlay is visible while a popup is open
      When the method "open" of "p" is called
      Then the property "ov.visible" of "root" is true

    Scenario: The overlay is hidden again when the last popup closes
      When the method "open" of "p" is called
      And the method "close" of "p" is called
      Then the property "ov.visible" of "root" is false

    Scenario: An open popup is a child of the overlay
      When the method "open" of "p" is called
      Then the property "contentItem.parent.parent.z" of "p" is 1000001

    Scenario: A popup whose parent is the overlay gets it once there is a window
      Given the file "tst_parent.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        import QtTest
        Item {
          id: root
          width: 300; height: 200
          Popup { id: p; parent: Overlay.overlay; width: 50; height: 50 }
          TestCase {
            name: "Parent"
            when: windowShown
            function test_parent() {
              verify(p.parent !== null)
              compare(p.parent, root.Overlay.overlay)
              compare(p.parent.width, 300)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Parent::test_parent()
        """

    Scenario: The overlay stays, hidden, after the last popup is destroyed
      Given the file "tst_stays.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        import QtTest
        Item {
          id: root
          width: 300; height: 200
          Component { id: made; Popup { width: 50; height: 50 } }
          TestCase {
            name: "Stays"
            when: windowShown
            function test_stays() {
              var p = made.createObject(root)
              p.open()
              var ov = root.Overlay.overlay
              verify(ov.visible)
              p.close()
              p.destroy()
              wait(0)
              compare(root.Overlay.overlay, ov)
              verify(!ov.visible)
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Stays::test_stays()
        """

    @api-QtQuick.Window.contentItem
    Scenario: The overlay's parent is the item the window's items hang from
      Given the file "tst_overlay_parent.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        import QtTest
        Item {
          id: root
          width: 300; height: 200
          TestCase {
            name: "OverlayParent"
            when: windowShown
            function test_parent() {
              var ov = root.Overlay.overlay
              compare(ov.parent, root.Window.contentItem)
              compare(ov.parent.width, 300)
              var at = ov.mapToItem(root, 0, 0)
              compare([at.x, at.y, ov.width, ov.height], [0, 0, 300, 200])
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : OverlayParent::test_parent()
        """

    # qmltestrunner takes an Item root only, so the oracle cannot run this
    # file; qml6 reads the same of an ApplicationWindow it shows (6.11.2,
    # offscreen).
    @ljos-only @api-QtQuick.Window.contentItem
    Scenario: In an application window the overlay's parent holds the bars too
      Given the file "tst_overlay_bars.qml":
        """qml
        import QtQuick
        import QtQuick.Controls
        import QtTest
        ApplicationWindow {
          id: w
          width: 300; height: 200
          visible: true
          header: Item { height: 20 }
          Item { id: a }
          TestCase {
            name: "OverlayBars"
            when: windowShown
            function test_parent() {
              var ov = a.Overlay.overlay
              compare(ov.parent, a.Window.contentItem)
              verify(a.Window.contentItem !== w.contentItem)
              compare([ov.y, ov.height], [0, 200])
            }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : OverlayBars::test_parent()
        """

  Rule: A popup that dims has a shade under it

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property var ov: Overlay.overlay
          property color shade: "transparent"
          property real shadeWidth: 0
          function look() {
            var shaded = ov.children.length > 1
            shade = shaded ? ov.children[0].color : "transparent"
            shadeWidth = shaded ? ov.children[0].width : 0
          }
          Popup { objectName: "p"; x: 100; y: 100; width: 100; height: 100 }
        }
        """

    Scenario: A popup that neither is modal nor dims has no shade
      When the method "open" of "p" is called
      And the method "look" of "root" is called
      Then the property "shadeWidth" of "root" is 0

    @api-QtQuick.Controls.Popup.modal
    Scenario: A modal popup darkens what is under it
      When the property "modal" of "p" is set to true
      And the method "open" of "p" is called
      And the method "look" of "root" is called
      Then the property "shade" of "root" is "#7f28282a"
      And the property "shadeWidth" of "root" is 400

    @api-QtQuick.Controls.Popup.dim
    Scenario: A popup that dims without being modal has a lighter shade
      When the property "dim" of "p" is set to true
      And the method "open" of "p" is called
      And the method "look" of "root" is called
      Then the property "shade" of "root" is "#1e28282a"

    Scenario: A modal popup dims unless it is told not to
      When the property "modal" of "p" is set to true
      Then the property "dim" of "p" is true

    Scenario: A modal popup that does not dim has no shade
      When the property "modal" of "p" is set to true
      And the property "dim" of "p" is set to false
      And the method "open" of "p" is called
      And the method "look" of "root" is called
      Then the property "shadeWidth" of "root" is 0

    Scenario: The shade goes when the popup closes
      When the property "modal" of "p" is set to true
      And the method "open" of "p" is called
      And the method "close" of "p" is called
      And the method "look" of "root" is called
      Then the property "shadeWidth" of "root" is 0

    Scenario: The shade follows the popup becoming modal while it is open
      When the property "dim" of "p" is set to true
      And the method "open" of "p" is called
      And the property "modal" of "p" is set to true
      And the method "look" of "root" is called
      Then the property "shade" of "root" is "#7f28282a"

  Rule: The shade can be replaced

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          objectName: "root"
          width: 400; height: 400
          property var ov: Overlay.overlay
          property color shade: "transparent"
          function look() {
            shade = ov.children.length > 1 ? ov.children[0].color : "transparent"
          }
          Popup {
            objectName: "p"
            width: 100; height: 100
            Overlay.modal: Rectangle { color: "#80ff0000" }
            Overlay.modeless: Rectangle { color: "#8000ff00" }
          }
        }
        """

    @api-QtQuick.Controls.Overlay.modal
    Scenario: A modal popup uses the component it is given for modal shades
      When the property "modal" of "p" is set to true
      And the method "open" of "p" is called
      And the method "look" of "root" is called
      Then the property "shade" of "root" is "#80ff0000"

    @api-QtQuick.Controls.Overlay.modeless
    Scenario: A dimming popup that is not modal uses the modeless one
      When the property "dim" of "p" is set to true
      And the method "open" of "p" is called
      And the method "look" of "root" is called
      Then the property "shade" of "root" is "#8000ff00"

  Rule: Popups are stacked in the order they were opened

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property string hit: ""
          Popup {
            objectName: "a"
            x: 50; y: 50; width: 100; height: 100; padding: 0
            closePolicy: Popup.NoAutoClose
            MouseArea { anchors.fill: parent; onPressed: root.hit += "a" }
          }
          Popup {
            objectName: "b"
            x: 100; y: 100; width: 100; height: 100; padding: 0
            closePolicy: Popup.NoAutoClose
            MouseArea { anchors.fill: parent; onPressed: root.hit += "b" }
          }
        }
        """

    Scenario: The popup opened last is on top
      When the method "open" of "b" is called
      And the method "open" of "a" is called
      And the user touches 125, 125
      Then the property "hit" of "root" is "a"

    Scenario: The order of declaration does not matter
      When the method "open" of "a" is called
      And the method "open" of "b" is called
      And the user touches 125, 125
      Then the property "hit" of "root" is "b"

    Scenario: A popup that is opened again comes to the top
      When the method "open" of "a" is called
      And the method "open" of "b" is called
      And the method "close" of "a" is called
      And the method "open" of "a" is called
      And the user touches 125, 125
      Then the property "hit" of "root" is "a"

    @api-QtQuick.Controls.Popup.z
    Scenario: A popup with a higher z is above one opened later
      When the property "z" of "a" is set to 1
      And the method "open" of "a" is called
      And the method "open" of "b" is called
      And the user touches 125, 125
      Then the property "hit" of "root" is "a"

    Scenario: A press on a popup does not reach the popup under it
      When the method "open" of "a" is called
      And the method "open" of "b" is called
      And the user touches 125, 125
      And the user touches 75, 75
      Then the property "hit" of "root" is "ba"
