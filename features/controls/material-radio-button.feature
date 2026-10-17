# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-material.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-radiobutton.html
# Verified on Qt 6.11.2 (qml6, offscreen): sizes and colours read from the
# buttons and their indicators.

@qt-parity
Feature: The Material RadioButton
  A Material radio button is 48 pixels tall: a ring 20 pixels across with
  a dot 10 across in it while it is checked. The ring is the secondary
  text colour when off, the accent when on, and the hint colour, as its
  text is, when disabled.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Rectangle {
        objectName: "root"; width: 200; height: 160; color: "white"
        readonly property Item offDot: off.indicator.children[0]
        readonly property Item onDot: on.indicator.children[0]
        RadioButton { id: off; objectName: "off"; text: "Ag" }
        RadioButton { id: on; objectName: "on"; y: 50; text: "Ag"; checked: true; autoExclusive: false }
        RadioButton { objectName: "dis"; y: 100; text: "Ag"; checked: true; enabled: false; autoExclusive: false }
      }
      """

  Scenario Outline: A Material radio button's <property> is <value>
    Then the property "<property>" of "<object>" is <value>

    Examples:
      | object | property                         | value       |
      | off    | implicitHeight                   | 48          |
      | off    | leftPadding                      | 8           |
      | off    | topPadding                       | 14          |
      | off    | spacing                          | 8           |
      | off    | indicator.x                      | 8           |
      | off    | indicator.y                      | 14          |
      | off    | indicator.width                  | 20          |
      | off    | indicator.radius                 | 10          |
      | off    | indicator.border.width           | 2           |
      | off    | indicator.border.color           | "#89000000" |
      | root   | offDot.scale                     | 0           |
      | on     | indicator.border.color           | "#e91e63"   |
      | root   | onDot.width                      | 10          |
      | root   | onDot.scale                      | 1           |
      | root   | onDot.color                      | "#e91e63"   |
      | dis    | indicator.border.color           | "#60000000" |
      | dis    | contentItem.color                | "#60000000" |
