# Sources:
#   https://doc.qt.io/qt-6/qml-color.html
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#rgba-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#hsla-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#hsva-method
#   https://doc.qt.io/qt-6/qcolor.html#operator-eq-eq
#   https://doc.qt.io/qt-6/qtqml-typesystem-valuetypes.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Colour values in JavaScript
  A colour read in an expression is a value with channels. It turns into text only where text is
  asked for.

  Scenario: A colour has red, green, blue and alpha channels from 0 to 1
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: "#80ff8000"
        property real red: color.r
        property int green: Math.round(color.g * 255)
        property real blue: color.b
        property int alpha: Math.round(color.a * 255)
      }
      """
    Then the property "red" of "root" is 1
    And the property "green" of "root" is 128
    And the property "blue" of "root" is 0
    And the property "alpha" of "root" is 128

  Scenario Outline: A colour has <member>
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: "#204080"
        property int part: Math.round(color.<member> * 1000)
      }
      """
    Then the property "part" of "root" is <thousandths>

    Examples:
      | member        | thousandths |
      | hsvHue        | 611         |
      | hsvSaturation | 750         |
      | hsvValue      | 502         |
      | hslHue        | 611         |
      | hslSaturation | 600         |
      | hslLightness  | 314         |

  Scenario: A colour is an object in an expression
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: "red"
        property string kind: typeof color
        property bool sized: color.length !== undefined
      }
      """
    Then the property "kind" of "root" is "object"
    And the property "sized" of "root" is false

  Scenario: A colour says whether it is valid
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: "red"
        property bool ok: color.valid
      }
      """
    Then the property "ok" of "root" is true

  Scenario: A colour becomes its hex text where text is asked for
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: "red"
        property string text: color
        property string joined: "is " + color
      }
      """
    Then the property "text" of "root" is "#ff0000"
    And the property "joined" of "root" is "is #ff0000"

  Scenario: A translucent colour is written with its alpha first
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string text: Qt.rgba(0.5, 0.25, 0, 0.5)
      }
      """
    Then the property "text" of "root" is "#80804000"

  Scenario: Two colours with the same channels are equal
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: "red"
        property color other: "#ff0000"
        property bool loosely: color == other
        property bool strictly: color === other
        property bool made: color === Qt.rgba(1, 0, 0, 1)
      }
      """
    Then the property "loosely" of "root" is true
    And the property "strictly" of "root" is true
    And the property "made" of "root" is true

  Scenario: Colours with different channels are not equal
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: "red"
        property bool differs: color != Qt.rgba(0, 0, 0, 1)
        property bool close: Qt.rgba(0.5, 0, 0, 1) == Qt.rgba(0.501, 0, 0, 1)
      }
      """
    Then the property "differs" of "root" is true
    And the property "close" of "root" is false

  Scenario: A colour equals its hex text loosely, and no other text
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: "red"
        property bool hex: color == "#ff0000"
        property bool strictHex: color === "#ff0000"
        property bool name: color == "red"
      }
      """
    Then the property "hex" of "root" is true
    And the property "strictHex" of "root" is false
    And the property "name" of "root" is false

  Scenario: Qt.rgba keeps more than eight bits a channel
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var made: Qt.rgba(0.5, 0.25, 0, 1)
        property int red: Math.round(made.r * 65535)
        property int green: Math.round(made.g * 65535)
      }
      """
    Then the property "red" of "root" is 32768
    And the property "green" of "root" is 16384

  Scenario: A colour kept in a var property is still a colour
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property var kept: Qt.rgba(0, 0, 1, 1)
        property real blue: kept.b
        property bool same: kept === Qt.rgba(0, 0, 1, 1)
      }
      """
    Then the property "blue" of "root" is 1
    And the property "same" of "root" is true

  Scenario: A colour is found in an array by its channels
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: "blue"
        property var palette: [Qt.rgba(1, 0, 0, 1), Qt.rgba(0, 0, 1, 1)]
        property int at: palette.indexOf(color)
      }
      """
    Then the property "at" of "root" is 1

  Scenario: Assigning what Qt.rgba gives changes the colour
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: "red"
        function paint() { color = Qt.rgba(0, 0, 1, 0.5) }
      }
      """
    When the method "paint" of "root" is called
    Then the property "color" of "root" is "#800000ff"

  Scenario: A binding follows the channel of a colour that changes
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: "red"
        property real blue: color.b
      }
      """
    When the property "color" of "root" is set to "#0000ff"
    Then the property "blue" of "root" is 1

  Scenario: Qt.rgba without an alpha gives an opaque colour
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: Qt.rgba(0, 0.5, 1)
      }
      """
    Then the property "color" of "root" is "#0080ff"

  Scenario: Qt.rgba keeps numbers outside 0 to 1 inside it
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: Qt.rgba(-1, 2, 0.5, 7)
      }
      """
    Then the property "color" of "root" is "#00ff80"

  Scenario: Qt.hsla makes a colour from hue, saturation and lightness
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: Qt.hsla(0.5, 1, 0.5, 1)
        property real red: color.r
        property real green: color.g
      }
      """
    Then the property "color" of "root" is "#00ffff"
    And the property "red" of "root" is 0
    And the property "green" of "root" is 1

  Scenario: Qt.hsva makes a colour from hue, saturation and value
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: Qt.hsva(0.25, 0.5, 0.75, 0.5)
      }
      """
    Then the property "color" of "root" is "#808fbf60"

  Scenario: Writing a channel changes the colour it was read from
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        color: "red"
        function tint() { color.b = 1 }
      }
      """
    When the method "tint" of "root" is called
    Then the property "color" of "root" is "#ff00ff"

  # A colour here is its four channels; it does not remember what it was made from.
  @backlog
  Scenario: A colour made from hue is not equal to one made from red, green and blue
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool same: Qt.hsla(0, 1, 0.5, 1) == Qt.rgba(1, 0, 0, 1)
        property bool sameText: "" + Qt.hsla(0, 1, 0.5, 1) === "" + Qt.rgba(1, 0, 0, 1)
      }
      """
    Then the property "same" of "root" is false
    And the property "sameText" of "root" is true

  # Found by ljos-fuzz (bindings), bucket value:color:default.
  Scenario: A colour with no alpha at all is written with its alpha
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string hsla: Qt.hsla(0, 0, 0, 0)
        property string rgba: Qt.rgba(0, 0, 0, 0)
      }
      """
    Then the property "hsla" of "root" is "#00000000"
    And the property "rgba" of "root" is "#00000000"

  # Found by ljos-fuzz (bindings), buckets warning:assign(ljos-only):literal and
  # value:bool:default.
  Scenario: A colour given an array is an invalid colour and is not reported
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property color c: []
        property int changes: 0
        onCChanged: changes++
        property bool ok: c.valid
        property string text: c
      }
      """
    Then the property "ok" of "root" is false
    And the property "text" of "root" is "#000000"
    And the property "changes" of "root" is 1
    And the log has no warning matching "assign"

  # Found by ljos-fuzz (bindings), bucket value:color:ref: Qt.lighter of a Text's colour
  # starts from the eight bits the Text kept.
  @backlog
  Scenario: A Text keeps eight bits a channel of its colour, a colour property more
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property color own: Qt.rgba(0, 0.5, 0, 1)
        Text { id: label; color: Qt.rgba(0, 0.5, 0, 1) }
        property bool textEight: Math.abs(label.color.g - 128 / 255) < 1e-6
        property bool ownEight: Math.abs(own.g - 128 / 255) < 1e-6
      }
      """
    Then the property "textEight" of "root" is true
    And the property "ownEight" of "root" is false
