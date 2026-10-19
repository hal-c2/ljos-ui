# Sources:
#   https://doc.qt.io/qt-6/qtquickcontrols-material.html
#   https://doc.qt.io/qt-6/qtquickcontrols-styles.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Material values reach what is made after them
  The Material attached properties an author gives an item propagate to
  the items below it, whenever those are made: the delegates a view
  makes as it scrolls, or the contents of a popup when it opens. Only
  what an author gives counts. Reading an item's Material values changes
  nothing for the items below it.

  Scenario: The delegates a view makes take the accent an item above gives
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Item {
        width: 200; height: 100
        Material.accent: Material.Red
        ListView {
          objectName: "list"; anchors.fill: parent
          model: 40
          delegate: Text {
            objectName: "row" + index; height: 20; text: index
            property color accent: Material.accent
          }
        }
      }
      """
    When the method "positionViewAtEnd" of "list" is called
    Then the property "accent" of "row39" is "#f44336"

  Scenario: An accent given after the items below were made reaches them
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Item {
        objectName: "root"
        function redden() { Material.accent = Material.Red }
        Item {
          Item {
            objectName: "leaf"
            property color accent: Material.accent
          }
        }
      }
      """
    Then the property "accent" of "leaf" is "#e91e63"
    When the method "redden" of "root" is called
    Then the property "accent" of "leaf" is "#f44336"

  Scenario: Reading the Material values of an item between changes nothing below it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Material
      Item {
        Material.accent: Material.Red
        Item {
          objectName: "middle"
          property color seen
          function look() { seen = Material.primary }
          Item {
            objectName: "leaf"
            property color accent: Material.accent
          }
        }
      }
      """
    When the method "look" of "middle" is called
    Then the property "seen" of "middle" is "#3f51b5"
    And the property "accent" of "leaf" is "#f44336"
