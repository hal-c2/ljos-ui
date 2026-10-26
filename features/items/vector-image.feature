# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-vectorimage-vectorimage.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A drawing from a file as items
  A VectorImage shows an SVG file as a drawing, not as pixels: it is
  as large as the file says, stretched to the item unless it is told
  otherwise, and kept to the item's top left corner when it is not.

  Background:
    Given the text file "wide.svg":
      """xml
      <svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20">
        <rect width="40" height="20" fill="#ff0000"/>
        <rect width="20" height="20" fill="#0000ff"/>
      </svg>
      """

  Scenario: A drawing is as large as its file says
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.VectorImage
      VectorImage { objectName: "drawing"; source: "wide.svg" }
      """
    Then the property "implicitWidth" of "drawing" is 40
    And the property "implicitHeight" of "drawing" is 20
    And the property "fillMode" of "drawing" is 3
    And the property "preferredRendererType" of "drawing" is 0
    And the log has no warnings

  Scenario Outline: How the drawing is fitted to the item
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.VectorImage
      Rectangle {
        width: 120; height: 120; color: "white"
        VectorImage {
          anchors.fill: parent
          source: "wide.svg"
          fillMode: VectorImage.<mode>
          preferredRendererType: VectorImage.CurveRenderer
        }
      }
      """
    Then the pixel at <x>, <y> of the window is "<colour>"

    Examples:
      | mode               | x   | y   | colour  |
      | Stretch            | 30  | 100 | #0000ff |
      | Stretch            | 90  | 100 | #ff0000 |
      | PreserveAspectFit  | 30  | 30  | #0000ff |
      | PreserveAspectFit  | 90  | 30  | #ff0000 |
      | PreserveAspectFit  | 60  | 90  | #ffffff |
      | PreserveAspectCrop | 110 | 100 | #0000ff |
      | NoResize           | 10  | 10  | #0000ff |
      | NoResize           | 30  | 10  | #ff0000 |
      | NoResize           | 60  | 60  | #ffffff |

  Scenario: A drawing made larger stays sharp
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.VectorImage
      Rectangle {
        width: 120; height: 60; color: "white"
        VectorImage { anchors.fill: parent; source: "wide.svg" }
      }
      """
    Then the pixel at 59, 30 of the window is "#0000ff"
    And the pixel at 60, 30 of the window is "#ff0000"

  # Qt says nothing of a drawing it cannot open; an Image says so, and an
  # application run without its downloaded files should say which are missing.
  @qt-diverges
  Scenario: A file that is not there leaves the item empty and says so
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.VectorImage
      VectorImage { objectName: "drawing"; source: "missing.svg" }
      """
    Then the property "implicitWidth" of "drawing" is 0
    And the property "implicitHeight" of "drawing" is 0
    And the log has a warning matching "QML Scenario: Cannot open: file:///.*missing\.svg$"
