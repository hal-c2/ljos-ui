# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#sourceSize-prop
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#paintedWidth-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A picture asked for at a size that follows from its own
  An Image whose "sourceSize" reads its own "height" asks for a size that
  depends on the size it is loaded at. Qt loads the picture again once,
  at the size the binding gave, and no more: the loop ends at the
  binding, while what the Image works out for itself (the size of the
  picture) is worked out whenever it is asked.

  Background:
    Given the text file "mark.svg":
      """xml
      <svg xmlns="http://www.w3.org/2000/svg" width="90" height="60" viewBox="0 0 90 60">
    <rect width="90" height="60" fill="#2cde85"/>
      </svg>
      """

  Scenario: A picture asked for no size is as large as the file says
    Given the QML:
      """qml
      import QtQuick
      Image { objectName: "plain"; source: "mark.svg" }
      """
    Then the property "width" of "plain" is 90
    And the property "height" of "plain" is 60
    And the property "sourceSize.width" of "plain" is 90
    And the property "sourceSize.height" of "plain" is 60
    And the property "paintedWidth" of "plain" is 90
    And the property "paintedHeight" of "plain" is 60

  Scenario: A width that is one and a half times the height asked for
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "loop"
        source: "mark.svg"
        sourceSize.width: height * 1.5
        sourceSize.height: 20
      }
      """
    Then the property "width" of "loop" is 30
    And the property "height" of "loop" is 20
    And the property "sourceSize.width" of "loop" is 30
    And the property "sourceSize.height" of "loop" is 20
    And the property "paintedWidth" of "loop" is 30
    And the property "paintedHeight" of "loop" is 20

  # Ljós ends this loop in the Image's own size, which it settles at 1 by 1
  # (sourceSize.height 0); Qt ends it at the binding and works the size out
  # again whenever it is asked: 60 by 40.
  @backlog
  Scenario: A width that follows the height when nothing else says how high
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "free"
        source: "mark.svg"
        sourceSize.width: height
      }
      """
    Then the property "width" of "free" is 60
    And the property "height" of "free" is 40
    And the property "sourceSize.width" of "free" is 60
    And the property "sourceSize.height" of "free" is 0
    And the property "paintedWidth" of "free" is 60
    And the property "paintedHeight" of "free" is 40

  Scenario: A picture in a layout asked for at three times its height
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Layouts
      RowLayout {
        height: 60
        Image {
          objectName: "laid"
          source: "mark.svg"
          Layout.alignment: Qt.AlignCenter
          sourceSize.width: height * 3
          sourceSize.height: 20
        }
      }
      """
    Then the property "width" of "laid" is 60
    And the property "height" of "laid" is 20
    And the property "sourceSize.width" of "laid" is 60
    And the property "sourceSize.height" of "laid" is 20
    And the property "paintedWidth" of "laid" is 60
    And the property "paintedHeight" of "laid" is 20
