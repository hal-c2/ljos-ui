# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-overlay.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#modal-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#dim-prop
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html#customizing-popup
# Verified on Qt 6.11.2 and 6.12.0 (qml6, offscreen).
#
# The colours are read as their parts and an alpha to two places: Qt
# 6.11.2 drops the fraction of an alpha (#7f…) and 6.12.0 rounds it (#80…).

@qt-parity @api-QtQuick.Controls.Overlay.modal
@api-QtQuick.Controls.Overlay.modeless
Feature: The shade of a popup is the style's
  What a popup that dims puts between itself and what is under it comes
  from its style. Each style has its own colour, the same for a popup, a
  dialog, a drawer and a menu, and a popup with no style has a shade that
  draws nothing. The shade goes clear as the popup starts to close.

  Rule: The Basic shade is the palette's shadow, half or an eighth

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          property var ov: Overlay.overlay
          property string shade: ""
          // The overlay's child that is no popup's own item.
          function shadeOf() {
            var own = [popup, dialog, drawer, menu].map(function (c) { return c.contentItem.parent })
            for (var i = 0; i < ov.children.length; ++i)
              if (own.indexOf(ov.children[i]) < 0)
                return ov.children[i]
            return null
          }
          function look() {
            var c = shadeOf().color
            shade = Math.round(c.r * 255) + " " + Math.round(c.g * 255) + " "
                + Math.round(c.b * 255) + " " + c.a.toFixed(2)
          }
          Popup { id: popup; objectName: "popup"; width: 100; height: 100 }
          Dialog { id: dialog; objectName: "dialog"; width: 100; height: 100 }
          Drawer { id: drawer; objectName: "drawer"; width: 100; height: 400 }
          Menu { id: menu; objectName: "menu"; MenuItem { text: "One" } }
        }
        """

    Scenario Outline: A modal <control> has the Basic modal shade
      When the property "modal" of "<control>" is set to true
      And the method "open" of "<control>" is called
      And the method "look" of "root" is called
      Then the property "shade" of "root" is "40 40 42 0.50"

      Examples:
        | control |
        | popup   |
        | dialog  |
        | drawer  |
        | menu    |

    Scenario Outline: A <control> that dims without being modal has the lighter Basic shade
      When the property "modal" of "<control>" is set to false
      And the property "dim" of "<control>" is set to true
      And the method "open" of "<control>" is called
      And the method "look" of "root" is called
      Then the property "shade" of "root" is "40 40 42 0.12"

      Examples:
        | control |
        | popup   |
        | dialog  |
        | drawer  |
        | menu    |

    Scenario: The Basic shade follows the palette's shadow
      When the property "palette.shadow" of "popup" is set to "red"
      And the property "modal" of "popup" is set to true
      And the method "open" of "popup" is called
      And the method "look" of "root" is called
      Then the property "shade" of "root" is "255 0 0 0.50"

  Rule: The Fusion shade is one faint black, modal or not

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Fusion
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          property var ov: Overlay.overlay
          property string shade: ""
          // The overlay's child that is no popup's own item.
          function shadeOf() {
            var own = [popup, dialog, drawer, menu].map(function (c) { return c.contentItem.parent })
            for (var i = 0; i < ov.children.length; ++i)
              if (own.indexOf(ov.children[i]) < 0)
                return ov.children[i]
            return null
          }
          function look() {
            var c = shadeOf().color
            shade = Math.round(c.r * 255) + " " + Math.round(c.g * 255) + " "
                + Math.round(c.b * 255) + " " + c.a.toFixed(2)
          }
          Popup { id: popup; objectName: "popup"; width: 100; height: 100 }
          Dialog { id: dialog; objectName: "dialog"; width: 100; height: 100 }
          Drawer { id: drawer; objectName: "drawer"; width: 100; height: 400 }
          Menu { id: menu; objectName: "menu"; MenuItem { text: "One" } }
        }
        """

    Scenario Outline: A <control> that is modal <modal> has the Fusion shade
      When the property "modal" of "<control>" is set to <modal>
      And the property "dim" of "<control>" is set to true
      And the method "open" of "<control>" is called
      And the method "look" of "root" is called
      Then the property "shade" of "root" is "0 0 0 0.07"

      Examples:
        | control | modal |
        | popup   | true  |
        | popup   | false |
        | dialog  | true  |
        | drawer  | true  |
        | menu    | false |

  Rule: The Material shade is light or dark with the theme

    Scenario Outline: A <control> in the <theme> theme that is modal <modal> has its shade
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Material
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          property var ov: Overlay.overlay
          property string shade: ""
          // The overlay's child that is no popup's own item.
          function shadeOf() {
            var own = [popup, dialog, drawer, menu].map(function (c) { return c.contentItem.parent })
            for (var i = 0; i < ov.children.length; ++i)
              if (own.indexOf(ov.children[i]) < 0)
                return ov.children[i]
            return null
          }
          function look() {
            var c = shadeOf().color
            shade = Math.round(c.r * 255) + " " + Math.round(c.g * 255) + " "
                + Math.round(c.b * 255) + " " + c.a.toFixed(2)
          }
          Popup { id: popup; objectName: "popup"; width: 100; height: 100; Material.theme: Material.<theme> }
          Dialog { id: dialog; objectName: "dialog"; width: 100; height: 100; Material.theme: Material.<theme> }
          Drawer { id: drawer; objectName: "drawer"; width: 100; height: 400; Material.theme: Material.<theme> }
          Menu { id: menu; objectName: "menu"; Material.theme: Material.<theme>; MenuItem { text: "One" } }
        }
        """
      When the property "modal" of "<control>" is set to <modal>
      And the property "dim" of "<control>" is set to true
      And the method "open" of "<control>" is called
      And the method "look" of "root" is called
      Then the property "shade" of "root" is "<shade>"

      Examples:
        | control | theme | modal | shade             |
        | popup   | Light | true  | 48 48 48 0.60     |
        | popup   | Light | false | 48 48 48 0.60     |
        | popup   | Dark  | true  | 250 250 250 0.60  |
        | dialog  | Light | true  | 48 48 48 0.60     |
        | drawer  | Dark  | true  | 250 250 250 0.60  |
        | menu    | Light | false | 48 48 48 0.60     |

  Rule: The Material shade fades in and out

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Material
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property var ov: Overlay.overlay
          property real shade: -1
          function look() {
            var shade = -1
            for (var i = 0; i < ov.children.length; ++i)
              if (ov.children[i] !== p.contentItem.parent)
                shade = ov.children[i].opacity
            root.shade = shade
          }
          Popup {
            id: p
            objectName: "p"
            width: 100; height: 100
            modal: true
            enter: null
            exit: Transition { NumberAnimation { duration: 600 } }
          }
        }
        """

    Scenario: The shade is clear as the popup opens
      When the method "open" of "p" is called
      And the method "look" of "root" is called
      Then the property "shade" of "root" is within 0.15 of 0

    Scenario: The shade has come in a little after the popup opened
      When the method "open" of "p" is called
      And 200 milliseconds pass
      And the method "look" of "root" is called
      Then the property "shade" of "root" is 1

    Scenario: The shade has gone out a little after the popup started to close
      When the method "open" of "p" is called
      And 200 milliseconds pass
      And the method "close" of "p" is called
      And 200 milliseconds pass
      And the method "look" of "root" is called
      Then the property "shade" of "root" is 0

  Rule: The Universal shade is the theme's base low colour

    Scenario Outline: A modal <control> in the <theme> theme has its shade
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Universal
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          property var ov: Overlay.overlay
          property string shade: ""
          // The overlay's child that is no popup's own item.
          function shadeOf() {
            var own = [popup, dialog, drawer, menu].map(function (c) { return c.contentItem.parent })
            for (var i = 0; i < ov.children.length; ++i)
              if (own.indexOf(ov.children[i]) < 0)
                return ov.children[i]
            return null
          }
          function look() {
            var c = shadeOf().color
            shade = Math.round(c.r * 255) + " " + Math.round(c.g * 255) + " "
                + Math.round(c.b * 255) + " " + c.a.toFixed(2)
          }
          Popup { id: popup; objectName: "popup"; width: 100; height: 100; Universal.theme: Universal.<theme> }
          Dialog { id: dialog; objectName: "dialog"; width: 100; height: 100; Universal.theme: Universal.<theme> }
          Drawer { id: drawer; objectName: "drawer"; width: 100; height: 400; Universal.theme: Universal.<theme> }
          Menu { id: menu; objectName: "menu"; Universal.theme: Universal.<theme>; MenuItem { text: "One" } }
        }
        """
      When the property "modal" of "<control>" is set to true
      And the method "open" of "<control>" is called
      And the method "look" of "root" is called
      Then the property "shade" of "root" is "<shade>"

      Examples:
        | control | theme | shade             |
        | popup   | Light | 0 0 0 0.20        |
        | popup   | Dark  | 255 255 255 0.20  |
        | dialog  | Light | 0 0 0 0.20        |
        | drawer  | Dark  | 255 255 255 0.20  |
        | menu    | Light | 0 0 0 0.20        |

  Rule: A popup with no shade of its style's has one that draws nothing

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Templates as T
        import QtQuick.Controls.Basic
        Item {
          id: root
          objectName: "root"
          width: 400; height: 400
          property var ov: Overlay.overlay
          property int count: 0
          property string colour: ""
          // What is in the overlay besides the popups' own items.
          function look() {
            var own = [plain, tip].map(function (c) { return c.contentItem.parent })
            var others = []
            for (var i = 0; i < ov.children.length; ++i)
              if (own.indexOf(ov.children[i]) < 0)
                others.push(ov.children[i])
            count = others.length
            colour = count > 0 ? "" + others[0].color : ""
          }
          T.Popup { id: plain; objectName: "plain"; width: 100; height: 100; contentItem: Item {} }
          ToolTip { id: tip; objectName: "tip"; text: "Tip"; timeout: -1 }
        }
        """

    Scenario Outline: A modal <control> with no shade of its style's has one with no colour
      When the property "modal" of "<control>" is set to true
      And the method "open" of "<control>" is called
      And the method "look" of "root" is called
      Then the property "count" of "root" is 1
      And the property "colour" of "root" is "undefined"

      Examples:
        | control |
        | plain   |
        | tip     |

    Scenario: A popup that dims without being modal, with no shade of its style's, has none
      When the property "dim" of "plain" is set to true
      And the method "open" of "plain" is called
      And the method "look" of "root" is called
      Then the property "count" of "root" is 0

  Rule: The shade goes clear as the popup starts to close

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 400; height: 400
          property var ov: Overlay.overlay
          property int count: 0
          property real shade: -1
          // What is in the overlay besides the popup's own item.
          function look() {
            var others = []
            for (var i = 0; i < ov.children.length; ++i)
              if (ov.children[i] !== p.contentItem.parent)
                others.push(ov.children[i])
            count = others.length
            shade = count > 0 ? others[0].opacity : -1
          }
          Popup {
            id: p
            objectName: "p"
            width: 100; height: 100
            modal: true
            exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: 600 } }
          }
        }
        """

    Scenario: The shade is clear while the popup goes
      When the method "open" of "p" is called
      And the method "close" of "p" is called
      And 300 milliseconds pass
      And the method "look" of "root" is called
      Then the property "count" of "root" is 1
      And the property "shade" of "root" is 0

    Scenario: The shade is gone once the popup has gone
      When the method "open" of "p" is called
      And the method "close" of "p" is called
      And 700 milliseconds pass
      And the method "look" of "root" is called
      Then the property "count" of "root" is 0
