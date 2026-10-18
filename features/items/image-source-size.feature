# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#sourceSize-prop
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#sourceClipRect-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Image source size
  "sourceSize" says how large the picture is kept in memory. The picture
  keeps its shape; the Image is as large as what was kept.

  Background:
    Given a 60 by 40 image file "wide.png" filled with "#208040"

  Scenario Outline: One side asked for, the other follows the picture's shape
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "picture"
        source: "wide.png"
        sourceSize.<side>: <asked>
      }
      """
    Then the property "implicitWidth" of "picture" is <width>
    And the property "implicitHeight" of "picture" is <height>

    Examples:
      | side   | asked | width | height |
      | width  | 30    | 30    | 20     |
      | height | 10    | 15    | 10     |
      | width  | 7     | 7     | 5      |
      | height | 7     | 11    | 7      |
      | width  | 120   | 60    | 40     |
      | height | 120   | 60    | 40     |

  Scenario Outline: Both sides asked for, the picture fits inside them
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "picture"
        source: "wide.png"
        sourceSize: Qt.size(<w>, <h>)
      }
      """
    Then the property "implicitWidth" of "picture" is <width>
    And the property "implicitHeight" of "picture" is <height>

    Examples:
      | w   | h   | width | height |
      | 30  | 30  | 30    | 20     |
      | 30  | 10  | 15    | 10     |
      | 10  | 30  | 10    | 7      |
      | 20  | 120 | 20    | 13     |
      | 120 | 120 | 60    | 40     |

  Scenario Outline: A picture that keeps its aspect ratio covers both sides, larger if need be
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "picture"
        source: "wide.png"
        fillMode: <mode>
        sourceSize: Qt.size(<w>, <h>)
      }
      """
    Then the property "implicitWidth" of "picture" is <width>
    And the property "implicitHeight" of "picture" is <height>

    Examples:
      | mode                     | w   | h   | width | height |
      | Image.PreserveAspectFit  | 30  | 10  | 30    | 20     |
      | Image.PreserveAspectCrop | 30  | 10  | 30    | 20     |
      | Image.PreserveAspectFit  | 70  | 50  | 75    | 50     |
      | Image.PreserveAspectCrop | 20  | 120 | 180   | 120    |
      | Image.PreserveAspectFit  | 120 | 0   | 120   | 80     |

  Scenario: The source size reads back what was asked for
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "picture"
        source: "wide.png"
        sourceSize.width: 30
      }
      """
    Then the property "sourceSize.width" of "picture" is 30
    And the property "sourceSize.height" of "picture" is 0

  Scenario: A source size set later loads the picture again at that size
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "picture"
        source: "wide.png"
        function shrink() { sourceSize = Qt.size(30, 0) }
      }
      """
    When the method "shrink" of "picture" is called
    Then the property "status" of "picture" is 1
    And the property "implicitWidth" of "picture" is 30
    And the property "implicitHeight" of "picture" is 20

  Scenario: A side bound to a width not yet known reads as bound, and asks once it is
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Image {
          objectName: "picture"; source: "wide.png"
          width: parent.width - 400; sourceSize.width: width
        }
      }
      """
    Then the property "sourceSize.width" of "picture" is -400
    And the property "sourceSize.height" of "picture" is 0
    And the property "implicitWidth" of "picture" is 60
    When the property "width" of "root" is set to 430
    Then the property "sourceSize.width" of "picture" is 30
    And the property "implicitWidth" of "picture" is 30
    And the property "implicitHeight" of "picture" is 20

  Scenario: Taking the source size back gives the whole picture
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "picture"
        source: "wide.png"
        sourceSize.width: 30
        function whole() { sourceSize = undefined }
      }
      """
    When the method "whole" of "picture" is called
    Then the property "implicitWidth" of "picture" is 60
    And the property "sourceSize.width" of "picture" is 60
    And the property "sourceSize.height" of "picture" is 40

  Scenario: A source clip rectangle keeps only that part
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "picture"
        source: "wide.png"
        sourceClipRect: Qt.rect(10, 5, 20, 12)
      }
      """
    Then the property "implicitWidth" of "picture" is 20
    And the property "implicitHeight" of "picture" is 12
    And the property "sourceSize.width" of "picture" is 20
    And the property "sourceSize.height" of "picture" is 12
