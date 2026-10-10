# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Image
  An Image shows a picture named by a URL. A picture in a local file is
  there as soon as the Image is made.

  Background:
    Given a 40 by 20 image file "red.png" filled with "#ff0000"
    And a 60 by 40 image file "blue.jpg" filled with "#0000ff"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 200
        Image { objectName: "picture"; source: "red.png" }
        Image { objectName: "empty" }
      }
      """

  Scenario Outline: Defaults
    Then the property "<property>" of "empty" is <value>

    Examples:
      | property            | value |
      | status              | 0     |
      | progress            | 0     |
      | fillMode            | 0     |
      | horizontalAlignment | 4     |
      | verticalAlignment   | 128   |
      | asynchronous        | false |
      | cache               | true  |
      | smooth              | true  |
      | mipmap              | false |
      | mirror              | false |
      | mirrorVertically    | false |
      | implicitWidth       | 0     |
      | implicitHeight      | 0     |
      | paintedWidth        | 0     |
      | paintedHeight       | 0     |
      | sourceSize.width    | 0     |
      | sourceSize.height   | 0     |

  Scenario: A picture in a local file is ready at once
    Then the property "status" of "picture" is 1
    And the property "progress" of "picture" is 1
    And the log has no warnings

  Scenario: The Image is as large as its picture unless told otherwise
    Then the property "implicitWidth" of "picture" is 40
    And the property "implicitHeight" of "picture" is 20
    And the property "width" of "picture" is 40
    And the property "height" of "picture" is 20

  Scenario: The source size reads the picture's size while none is asked for
    Then the property "sourceSize.width" of "picture" is 40
    And the property "sourceSize.height" of "picture" is 20

  Scenario: A JPEG file loads as a PNG file does
    When the property "source" of "empty" is set to "blue.jpg"
    Then the property "status" of "empty" is 1
    And the property "implicitWidth" of "empty" is 60
    And the property "implicitHeight" of "empty" is 40

  Scenario: Another source replaces the picture
    When the property "source" of "picture" is set to "blue.jpg"
    Then the property "status" of "picture" is 1
    And the property "implicitWidth" of "picture" is 60
    And the property "width" of "picture" is 60

  Scenario: An empty source leaves nothing
    When the property "source" of "picture" is set to ""
    Then the property "status" of "picture" is 0
    And the property "progress" of "picture" is 0
    And the property "implicitWidth" of "picture" is 0
    And the property "implicitHeight" of "picture" is 0

  Scenario: A width that was set stays when the picture changes
    When the property "width" of "picture" is set to 100
    And the property "source" of "picture" is set to "blue.jpg"
    Then the property "width" of "picture" is 100
    And the property "height" of "picture" is 40

  Scenario: An Image that is not visible loads its picture all the same
    When the property "visible" of "empty" is set to false
    And the property "source" of "empty" is set to "red.png"
    Then the property "status" of "empty" is 1
    And the property "implicitWidth" of "empty" is 40

  Scenario: A file that is not there is an error and a warning
    When the property "source" of "picture" is set to "missing.png"
    Then the property "status" of "picture" is 3
    And the property "progress" of "picture" is 0
    And the property "implicitWidth" of "picture" is 0
    And the log has a warning matching "Scenario.qml:5:9: QML Image: Cannot open: file:///.*missing\.png$"

  Scenario: A file that is not an image is an error and a warning
    Given a file "letter.png" that is not an image
    When the property "source" of "picture" is set to "letter.png"
    Then the property "status" of "picture" is 3
    And the property "implicitWidth" of "picture" is 0
    And the log has a warning matching "QML Image: Error decoding: file:///.*letter\.png: Unsupported image format$"

  Scenario: A good source after a bad one clears the error
    When the property "source" of "picture" is set to "missing.png"
    And the property "source" of "picture" is set to "red.png"
    Then the property "status" of "picture" is 1
    And the property "implicitWidth" of "picture" is 40

  Scenario: Mirroring and caching do not change the size
    When the property "mirror" of "picture" is set to true
    And the property "mirrorVertically" of "picture" is set to true
    And the property "cache" of "picture" is set to false
    Then the property "implicitWidth" of "picture" is 40
    And the property "paintedWidth" of "picture" is 40
    And the property "status" of "picture" is 1
