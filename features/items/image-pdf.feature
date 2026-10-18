# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#currentFrame-prop
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#frameCount-prop
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#sourceSize-prop
# Verified on Qt 6.11.2 (qml6, offscreen), with the four pages of
# photosurface's welcome.pdf and with files like those made here: a page is
# as large as its media box in points, rounded; "currentFrame" set later
# to a page there is not is ignored, while one set at the start stays and
# shows the first page; frameCount is set after status turns Ready.

@qt-parity
Feature: PDF documents in an Image
  An Image shows a page of a PDF document, as a drawing: "frameCount" is
  how many pages it has and "currentFrame" the page shown, the first
  unless another is asked. A page is drawn at its own size in points, or
  at the size "sourceSize" asks, over nothing, so that what is behind the
  Image shows around what the page paints.

  Background:
    Given a PDF file "pages.pdf" of pages "300 by 200, 150 by 400, 100.6 by 50.4" painted "#ff0000"

  @api-QtQuick.Image.frameCount
  Scenario: A PDF document counts its pages and shows the first
    Given the QML:
      """qml
      import QtQuick
      Image { objectName: "picture"; source: "pages.pdf" }
      """
    Then the property "status" of "picture" is 1
    And the property "frameCount" of "picture" is 3
    And the property "currentFrame" of "picture" is 0
    And the property "implicitWidth" of "picture" is 300
    And the property "implicitHeight" of "picture" is 200
    And the property "sourceSize.width" of "picture" is 300
    And the property "sourceSize.height" of "picture" is 200
    And the log has no warnings

  @api-QtQuick.Image.currentFrame
  Scenario Outline: Another page is as large as it is, rounded to whole pixels
    Given the QML:
      """qml
      import QtQuick
      Image { objectName: "picture"; source: "pages.pdf"; currentFrame: <page> }
      """
    Then the property "implicitWidth" of "picture" is <width>
    And the property "implicitHeight" of "picture" is <height>

    Examples:
      | page | width | height |
      | 1    | 150   | 400    |
      | 2    | 101   | 50     |

  Scenario: Turning to another page shows it at its size
    Given the QML:
      """qml
      import QtQuick
      Image { objectName: "picture"; source: "pages.pdf" }
      """
    When the property "currentFrame" of "picture" is set to 1
    Then the property "currentFrame" of "picture" is 1
    And the property "sourceSize.width" of "picture" is 150
    And the property "implicitHeight" of "picture" is 400

  Scenario Outline: Turning to a page there is not is ignored
    Given the QML:
      """qml
      import QtQuick
      Image { objectName: "picture"; source: "pages.pdf"; currentFrame: 1 }
      """
    When the property "currentFrame" of "picture" is set to <page>
    Then the property "currentFrame" of "picture" is 1
    And the property "implicitWidth" of "picture" is 150

    Examples:
      | page |
      | 3    |
      | -1   |

  Scenario Outline: A page asked at another width keeps its shape
    Given the QML:
      """qml
      import QtQuick
      Image { objectName: "picture"; source: "pages.pdf"; sourceSize.width: <asked> }
      """
    Then the property "implicitWidth" of "picture" is <asked>
    And the property "implicitHeight" of "picture" is <height>

    Examples:
      | asked | height |
      | 600   | 400    |
      | 150   | 100    |

  Scenario: What the page does not paint shows what is behind it
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 300; height: 200; color: "#0000ff"
        Image { source: "pages.pdf" }
      }
      """
    Then the pixel at 150, 100 of the window is "#ff0000"
    And the pixel at 20, 20 of the window is "#0000ff"
