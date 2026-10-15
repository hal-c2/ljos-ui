# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-screen.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The attached Screen
  `Screen.` read in any object says what is known of the screen it is
  shown on: an item's is its window's screen, and an object outside a
  window has the primary screen. The offscreen platform has one screen of
  800 by 800 with 96 logical pixels to the inch.

  @api-QtQuick.Screen.width @api-QtQuick.Screen.height @api-QtQuick.Screen.primaryOrientation @api-QtQuick.Screen.orientation @api-QtQuick.Screen.logicalPixelDensity @api-QtQuick.Screen.desktopAvailableWidth @api-QtQuick.Screen.desktopAvailableHeight @api-QtQuick.Screen.virtualX @api-QtQuick.Screen.virtualY @api-QtQuick.Screen.devicePixelRatio
  Scenario Outline: An object outside a window has the primary screen
    Given the QML:
      """qml
      import QtQuick
      QtObject {
        objectName: "root"
        readonly property int width: Screen.width
        readonly property int height: Screen.height
        readonly property int primary: Screen.primaryOrientation
        readonly property bool landscape: Screen.primaryOrientation === Qt.LandscapeOrientation
        readonly property int orientation: Screen.orientation
        readonly property real logical: Screen.logicalPixelDensity
        readonly property int availableWidth: Screen.desktopAvailableWidth
        readonly property int availableHeight: Screen.desktopAvailableHeight
        readonly property int x: Screen.virtualX
        readonly property int y: Screen.virtualY
        readonly property real ratio: Screen.devicePixelRatio
      }
      """
    Then the property "<property>" of "root" is <value>

    Examples:
      | property        | value |
      | width           | 800   |
      | height          | 800   |
      | primary         | 2     |
      | landscape       | true  |
      | orientation     | 0     |
      | availableWidth  | 800   |
      | availableHeight | 800   |
      | x               | 0     |
      | y               | 0     |
      | ratio           | 1     |

  @api-QtQuick.Screen.logicalPixelDensity
  Scenario: The logical pixel density is 96 to the inch
    Given the QML:
      """qml
      import QtQuick
      QtObject {
        objectName: "root"
        readonly property real logical: Screen.logicalPixelDensity
      }
      """
    Then the property "logical" of "root" is within 0.001 of 3.7795

  @api-QtQuick.Screen.name @api-QtQuick.Screen.manufacturer @api-QtQuick.Screen.model @api-QtQuick.Screen.serialNumber @api-QtQuick.Screen.pixelDensity @api-QtQuick.Window.screen
  Scenario: An item has its window's screen
    Given the QML:
      """qml
      import QtQuick
      Window {
        id: w
        width: 300; height: 200
        visible: true
        Item {
          objectName: "inner"
          readonly property var pairs: [
            ["name", Screen.name, w.screen.name], ["model", Screen.model, w.screen.model],
            ["manufacturer", Screen.manufacturer, w.screen.manufacturer],
            ["serialNumber", Screen.serialNumber, w.screen.serialNumber],
            ["width", Screen.width, w.screen.width], ["height", Screen.height, w.screen.height],
            ["desktopAvailableWidth", Screen.desktopAvailableWidth, w.screen.desktopAvailableWidth],
            ["desktopAvailableHeight", Screen.desktopAvailableHeight, w.screen.desktopAvailableHeight],
            ["logicalPixelDensity", Screen.logicalPixelDensity, w.screen.logicalPixelDensity],
            ["pixelDensity", Screen.pixelDensity, w.screen.pixelDensity],
            ["devicePixelRatio", Screen.devicePixelRatio, w.screen.devicePixelRatio],
            ["primaryOrientation", Screen.primaryOrientation, w.screen.primaryOrientation],
            ["orientation", Screen.orientation, w.screen.orientation],
            ["virtualX", Screen.virtualX, w.screen.virtualX], ["virtualY", Screen.virtualY, w.screen.virtualY]]
          readonly property string differs: pairs.filter(p => p[1] !== p[2]).map(p => p[0]).join(",")
          readonly property bool known: Screen.width > 0
        }
      }
      """
    Then the property "differs" of "inner" is ""
    And the property "known" of "inner" is true

  @api-QtQuick.Screen.angleBetween
  Scenario Outline: The angle between two orientations
    Given the QML:
      """qml
      import QtQuick
      QtObject {
        objectName: "root"
        property int got: -1
        function angle(a, b) { got = Screen.angleBetween(a, b) }
      }
      """
    When the method "angle" of "root" is called with <a>, <b>
    Then the property "got" of "root" is <angle>

    Examples:
      | a | b | angle |
      | 1 | 2 | 270   |
      | 2 | 1 | 90    |
      | 2 | 8 | 180   |
      | 2 | 4 | 270   |
      | 4 | 4 | 0     |
      | 0 | 1 | 90    |
      | 8 | 1 | 270   |
