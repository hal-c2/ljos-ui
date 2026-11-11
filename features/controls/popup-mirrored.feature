# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#mirrored-prop
#   https://doc.qt.io/qt-6/qml-qtquick-layoutmirroring.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-overlay.html
# Verified on Qt 6.11.2 and 6.12.0 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.Popup.mirrored
Feature: A popup is mirrored with the window it is shown in
  A popup is shown in the window's overlay, which lies inside the window's
  content. A window that passes its mirroring on to its children mirrors
  the popups shown in it too, wherever in the window they were written.
  An item that passes its mirroring on does not: the overlay is not inside it.

  Rule: A popup takes the mirroring a window passes on
    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Window {
          objectName: "window"
          width: 400; height: 400
          property bool rtl: true
          property bool inherit: true
          LayoutMirroring.enabled: rtl
          LayoutMirroring.childrenInherit: inherit
          Popup { objectName: "popup"; width: 100; height: 50 }
          Menu { objectName: "menu"; width: 120; MenuItem { text: "a" } }
          Dialog { objectName: "dialog"; width: 100; height: 50 }
          Drawer { objectName: "drawer"; width: 100; height: 400 }
          Item {
            Popup { objectName: "inner"; width: 100; height: 50 }
          }
        }
        """

    Scenario Outline: A <control> in a window that passes mirroring on is mirrored
      When the method "open" of "<control>" is called
      Then the property "mirrored" of "<control>" is true

      Examples:
        | control |
        | popup   |
        | menu    |
        | dialog  |
        | drawer  |
        | inner   |

    Scenario: A popup is not mirrored by a window that keeps its mirroring to itself
      When the property "inherit" of "window" is set to false
      And the method "open" of "popup" is called
      Then the property "mirrored" of "popup" is false

    Scenario: A shown popup follows the window's mirroring as it changes
      When the method "open" of "popup" is called
      And the property "rtl" of "window" is set to false
      Then the property "mirrored" of "popup" is false
      When the property "rtl" of "window" is set to true
      Then the property "mirrored" of "popup" is true

  Rule: An item does not pass its mirroring on to a popup
    Scenario: A popup written in a mirrored item is not mirrored
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 400; height: 400
          LayoutMirroring.enabled: true
          LayoutMirroring.childrenInherit: true
          Popup { objectName: "popup"; width: 100; height: 50 }
        }
        """
      When the method "open" of "popup" is called
      Then the property "mirrored" of "popup" is false

    Scenario: A popup written in a mirrored item of a window is not mirrored by that item
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Window {
          width: 400; height: 400
          Item {
            LayoutMirroring.enabled: true
            LayoutMirroring.childrenInherit: true
            Popup { objectName: "popup"; width: 100; height: 50 }
          }
        }
        """
      When the method "open" of "popup" is called
      Then the property "mirrored" of "popup" is false
