# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#sourceSize-prop
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#source-prop
#   https://doc.qt.io/qt-6/qimagereader.html#supportedImageFormats
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: SVG pictures in an Image
  An SVG file is a drawing, not pixels: it is drawn at the size the Image
  asks for in "sourceSize", or at the size the file gives itself when
  nothing is asked. Unlike a PNG it is made larger as readily as smaller.

  Background:
    Given the text file "wide.svg":
      """xml
      <svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20">
        <rect width="40" height="20" fill="#ff0000"/>
        <rect width="20" height="20" fill="#0000ff"/>
      </svg>
      """

  @api-QtQuick.Image.source
  Scenario: An SVG file is as large as it says
    Given the QML:
      """qml
      import QtQuick
      Image { objectName: "picture"; source: "wide.svg" }
      """
    Then the property "status" of "picture" is 1
    And the property "progress" of "picture" is 1
    And the property "implicitWidth" of "picture" is 40
    And the property "implicitHeight" of "picture" is 20
    And the property "sourceSize.width" of "picture" is 40
    And the property "sourceSize.height" of "picture" is 20
    And the log has no warnings

  Scenario: Without a width and height the view box gives the size
    Given the text file "box.svg":
      """xml
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 30 10">
        <rect width="30" height="10" fill="#00ff00"/>
      </svg>
      """
    And the QML:
      """qml
      import QtQuick
      Image { objectName: "picture"; source: "box.svg" }
      """
    Then the property "implicitWidth" of "picture" is 30
    And the property "implicitHeight" of "picture" is 10

  @api-QtQuick.Image.sourceSize
  Scenario Outline: One side asked for, the other follows the drawing's shape, larger or smaller
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "picture"
        source: "wide.svg"
        sourceSize.<side>: <asked>
      }
      """
    Then the property "implicitWidth" of "picture" is <width>
    And the property "implicitHeight" of "picture" is <height>
    And the property "sourceSize.<side>" of "picture" is <asked>

    Examples:
      | side   | asked | width | height |
      | width  | 80    | 80    | 40     |
      | width  | 10    | 10    | 5      |
      | height | 5     | 10    | 5      |
      | height | 30    | 60    | 30     |

  Scenario Outline: Both sides asked for, the drawing is made exactly that size
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "picture"
        source: "wide.svg"
        sourceSize: Qt.size(<w>, <h>)
      }
      """
    Then the property "implicitWidth" of "picture" is <w>
    And the property "implicitHeight" of "picture" is <h>

    Examples:
      | w  | h  |
      | 80 | 80 |
      | 10 | 30 |
      | 40 | 20 |

  Scenario Outline: A fill mode that keeps the shape covers both sides
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "picture"
        source: "wide.svg"
        fillMode: <mode>
        sourceSize: Qt.size(<w>, <h>)
      }
      """
    Then the property "implicitWidth" of "picture" is <width>
    And the property "implicitHeight" of "picture" is <height>

    Examples:
      | mode                     | w  | h  | width | height |
      | Image.PreserveAspectFit  | 80 | 80 | 160   | 80     |
      | Image.PreserveAspectCrop | 80 | 80 | 160   | 80     |
      | Image.PreserveAspectFit  | 20 | 30 | 60    | 30     |

  Scenario Outline: A drawing made to a size of another shape is stretched to it
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 100; height: 100; color: "white"
        Image { source: "wide.svg"; sourceSize: Qt.size(80, 80) }
      }
      """
    Then the pixel at <x>, <y> of the window is "<colour>"

    Examples:
      | x  | y  | colour  |
      | 5  | 5  | #0000ff |
      | 35 | 75 | #0000ff |
      | 45 | 5  | #ff0000 |
      | 75 | 75 | #ff0000 |
      | 90 | 40 | #ffffff |
      | 40 | 90 | #ffffff |

  Scenario: An Image larger than its drawing fits the drawing in it
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 100; height: 100; color: "white"
        Image {
          objectName: "picture"
          width: 100; height: 100
          source: "wide.svg"
          fillMode: Image.PreserveAspectFit
        }
      }
      """
    Then the property "paintedWidth" of "picture" is 100
    And the property "paintedHeight" of "picture" is 50
    And the pixel at 10, 50 of the window is "#0000ff"
    And the pixel at 90, 50 of the window is "#ff0000"
    And the pixel at 50, 10 of the window is "#ffffff"

  Scenario: A source clip rectangle cuts the drawing after it was made to size
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "picture"
        source: "wide.svg"
        sourceSize.width: 80
        sourceClipRect: Qt.rect(10, 0, 20, 10)
      }
      """
    Then the property "implicitWidth" of "picture" is 20
    And the property "implicitHeight" of "picture" is 10
    And the property "sourceSize.width" of "picture" is 80

  @api-QtQuick.Image.asynchronous
  Scenario: An SVG loaded in the background arrives a moment later
    Given the QML:
      """qml
      import QtQuick
      Image { objectName: "picture"; asynchronous: true; source: "wide.svg" }
      """
    Then the property "status" of "picture" is 2
    When 200 milliseconds pass
    Then the property "status" of "picture" is 1
    And the property "implicitWidth" of "picture" is 40

  Scenario: An SVG is known by what it holds, not by its name
    Given the text file "drawing.png":
      """xml
      <svg xmlns="http://www.w3.org/2000/svg" width="12" height="8"/>
      """
    And the QML:
      """qml
      import QtQuick
      Image { objectName: "picture"; source: "drawing.png" }
      """
    Then the property "status" of "picture" is 1
    And the property "implicitWidth" of "picture" is 12
    And the property "implicitHeight" of "picture" is 8

  Scenario: An SVG that ends too soon is an error and a warning
    Given the text file "cut.svg":
      """xml
      <svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40"
      """
    And the QML:
      """qml
      import QtQuick
      Image { objectName: "picture"; source: "cut.svg" }
      """
    Then the property "status" of "picture" is 3
    And the property "implicitWidth" of "picture" is 0
    And the log has a warning matching "Error decoding: file:///.*cut\.svg: Unable to read image data$"

  Scenario: A file named as an SVG that is no drawing is an unsupported format
    Given the text file "letter.svg":
      """xml
      Dear reader,
      """
    And the QML:
      """qml
      import QtQuick
      Image { objectName: "picture"; source: "letter.svg" }
      """
    Then the property "status" of "picture" is 3
    And the log has a warning matching "Error decoding: file:///.*letter\.svg: Unsupported image format$"

  Scenario: A picture the SVG file embeds is drawn with it
    Given the text file "logo.svg":
      """xml
      <svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="40" height="20">
        <image width="40" height="20" preserveAspectRatio="none" xlink:href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAIAAAAmkwkpAAAAEElEQVR42mNQSFgARwzEcQDMQxIBWLUzWAAAAABJRU5ErkJggg=="/>
      </svg>
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        width: 40; height: 20
        Image { source: "logo.svg" }
      }
      """
    Then the pixel at 20, 10 of the window is "#2060a0"
    And the log has no warnings

  Rule: A source without a suffix is looked for with the suffix of each picture format

    Scenario: A name without a suffix finds the SVG file
      Given the QML:
        """qml
        import QtQuick
        Image { objectName: "picture"; source: "wide" }
        """
      Then the property "status" of "picture" is 1
      And the property "implicitWidth" of "picture" is 40

    Scenario: The format first in alphabetical order wins
      Given a 12 by 6 image file "wide.png" filled with "#00ff00"
      And the QML:
        """qml
        import QtQuick
        Image { objectName: "picture"; source: "wide" }
        """
      Then the property "implicitWidth" of "picture" is 12

    Scenario: A JPEG comes before a PNG
      Given a 12 by 6 image file "logo.png" filled with "#00ff00"
      And a 14 by 7 image file "logo.jpg" filled with "#00ff00"
      And the QML:
        """qml
        import QtQuick
        Image { objectName: "picture"; source: "logo" }
        """
      Then the property "implicitWidth" of "picture" is 14

    Scenario: A name with a suffix is not looked for with another
      Given the QML:
        """qml
        import QtQuick
        Image { objectName: "picture"; source: "wide.png" }
        """
      Then the property "status" of "picture" is 3
      And the log has a warning matching "Cannot open: file://.*/wide\.png$"

    Scenario: A name that matches no file is an error under that name
      Given the QML:
        """qml
        import QtQuick
        Image { objectName: "picture"; source: "missing" }
        """
      Then the property "status" of "picture" is 3
      And the log has a warning matching "Cannot open: file://.*/missing$"
