# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-palette.html
#   https://doc.qt.io/qt-6/qml-qtquick-colorgroup.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#palette-prop
# Verified on Qt 6.11.2 (qml6, offscreen), in the Basic style.

@qt-parity @api-QtQuick.Palette @api-QtQuick.ColorGroup
Feature: A whole palette or colour group assigned at once
  A palette is given whole as well as a colour at a time: a "Palette"
  object, another item's palette, or one of its colour groups. What is
  given is what was set there, in each group; what was not set is the
  colour from above again, as if the item had set nothing. Assigning
  "undefined" to a palette or a group takes back every colour set in
  it. A palette given in QML follows the one it names; one assigned
  from a script is a copy of it as it is then. A colour group given
  from another palette's group is that palette's group of the same name
  as the one given: "palette.disabled: a.palette.active" takes the
  disabled colours of "a", as qml6 does.

  Scenario: A Palette object gives the colours set in it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Button {
        objectName: "b"
        palette: Palette { button: "#ff0000"; buttonText: "#00ff00"; disabled.button: "#0000ff" }
      }
      """
    Then the property "palette.button" of "b" is "#ff0000"
    And the property "palette.buttonText" of "b" is "#00ff00"
    And the property "palette.window" of "b" is "#ffffff"
    And the property "palette.active.button" of "b" is "#ff0000"
    And the property "palette.inactive.button" of "b" is "#ff0000"
    And the property "palette.disabled.button" of "b" is "#0000ff"

  Scenario: Another item's palette gives what was set on it and follows it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        Button { id: a; objectName: "a"; palette.button: "#ff0000"; palette.disabled.buttonText: "#0000ff" }
        Item {
          palette.window: "#336699"
          palette.buttonText: "#445566"
          Button { objectName: "b"; palette: a.palette }
        }
      }
      """
    Then the property "palette.button" of "b" is "#ff0000"
    And the property "palette.buttonText" of "b" is "#445566"
    And the property "palette.window" of "b" is "#336699"
    And the property "palette.disabled.buttonText" of "b" is "#0000ff"
    When the property "palette.window" of "a" is set to "#00ff00"
    Then the property "palette.window" of "b" is "#00ff00"
    And the property "palette.inactive.window" of "b" is "#00ff00"

  Scenario: A colour group given from another item's group
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        Button { id: a; objectName: "a"; palette.button: "#ff0000"; palette.disabled.buttonText: "#0000ff" }
        Item {
          palette.buttonText: "#445566"
          Button { objectName: "c"; palette.active: a.palette.active }
          Button { objectName: "d"; enabled: false; palette.disabled: a.palette.disabled }
        }
      }
      """
    Then the property "palette.button" of "c" is "#ff0000"
    And the property "palette.inactive.button" of "c" is "#e0e0e0"
    And the property "palette.disabled.buttonText" of "c" is "#445566"
    And the property "palette.buttonText" of "d" is "#0000ff"
    And the property "palette.button" of "d" is "#ff0000"
    And the property "palette.active.button" of "d" is "#e0e0e0"

  Scenario: A palette assigned from a script is a copy that replaces the own colours
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        function copy() { d.palette = a.palette }
        Button { id: a; objectName: "a"; palette.button: "#ff0000"; palette.disabled.buttonText: "#0000ff" }
        Item {
          palette.window: "#336699"
          Button { id: d; objectName: "d"; palette.base: "#111111" }
        }
      }
      """
    When the method "copy" of "root" is called
    Then the property "palette.button" of "d" is "#ff0000"
    And the property "palette.disabled.buttonText" of "d" is "#0000ff"
    And the property "palette.base" of "d" is "#ffffff"
    And the property "palette.window" of "d" is "#336699"
    When the property "palette.window" of "a" is set to "#00ff00"
    Then the property "palette.window" of "d" is "#336699"

  Scenario: A colour group assigned from a script takes the group of its own name
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        function copy() { d.palette.disabled = a.palette.active }
        Button { id: a; palette.disabled.button: "#0000ff"; palette.active.base: "#00ffff" }
        Button { id: d; objectName: "d"; palette.active.text: "#123456" }
      }
      """
    When the method "copy" of "root" is called
    Then the property "palette.disabled.button" of "d" is "#0000ff"
    And the property "palette.disabled.base" of "d" is "#d6d6d6"
    And the property "palette.active.base" of "d" is "#ffffff"
    And the property "palette.active.text" of "d" is "#123456"

  Scenario: Assigning undefined takes back the whole palette
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        function clear() { b.palette = undefined }
        palette.buttonText: "#445566"
        Button { id: b; objectName: "b"; palette.button: "#ff0000"; palette.buttonText: "#00ff00"; palette.disabled.button: "#0000ff" }
      }
      """
    When the method "clear" of "root" is called
    Then the property "palette.button" of "b" is "#e0e0e0"
    And the property "palette.buttonText" of "b" is "#445566"
    And the property "palette.disabled.button" of "b" is "#e0e0e0"

  Scenario: Assigning undefined to a group takes back that group alone
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        function clear() { b.palette.active = undefined }
        Button { id: b; objectName: "b"; palette.active.button: "#ff0000"; palette.disabled.button: "#0000ff" }
      }
      """
    When the method "clear" of "root" is called
    Then the property "palette.button" of "b" is "#e0e0e0"
    And the property "palette.active.button" of "b" is "#e0e0e0"
    And the property "palette.disabled.button" of "b" is "#0000ff"

  Scenario: Taking back a group takes the palette's colours out of it alone
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        function clear() { b.palette.active = undefined }
        Button { id: b; objectName: "b"; palette.button: "#ff0000"; palette.active.base: "#00ffff" }
      }
      """
    When the method "clear" of "root" is called
    Then the property "palette.active.button" of "b" is "#e0e0e0"
    And the property "palette.active.base" of "b" is "#ffffff"
    And the property "palette.inactive.button" of "b" is "#ff0000"
    And the property "palette.disabled.button" of "b" is "#ff0000"

  Scenario: A colour set on the palette later is set over the one set in a group
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        function paint() { b.palette.button = "#abcdef" }
        Button { id: b; objectName: "b"; palette.button: "#ff0000"; palette.disabled.button: "#0000ff" }
      }
      """
    When the method "paint" of "root" is called
    Then the property "palette.button" of "b" is "#abcdef"
    And the property "palette.disabled.button" of "b" is "#abcdef"

  Scenario: A colour group given in "onCompleted" is what the control reads there
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        property color seen
        Button { id: a; palette.active.base: "#00ffff" }
        Button { id: b }
        Component.onCompleted: { b.palette.active = a.palette.active; seen = b.palette.base }
      }
      """
    Then the property "seen" of "root" is "#00ffff"
