# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#cursorRectangle-prop
# Verified on Qt 6.11.2 (qml6, offscreen): after "h" in 14 px Liberation
# Sans the cursor's rectangle is at x 7.78125, a pixel wide, and columns
# 17 and 18 of the window are black; at x 0 only column 10 is.

@qt-parity @api-QtQuick.TextInput.cursorRectangle
Feature: The cursor is drawn on whole pixels
  The cursor's rectangle lies where the text puts it, between pixels as
  often as not. It is drawn solid in the colour of the text over every
  pixel the rectangle touches, not blended into the ones it partly
  covers, so that it reads as sharp wherever it stands.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 60; color: "white"
        TextInput {
          objectName: "input"
          x: 10; y: 10; width: 180; height: 20
          font.family: "Liberation Sans"; font.pixelSize: 14
          text: "h"
        }
      }
      """
    When the user presses the pointer at 11, 25
    And the user releases the pointer at 11, 25

  Scenario: A cursor between pixels covers both, solid
    When the user presses "End"
    And 50 milliseconds pass
    Then the property "cursorRectangle.x" of "input" is 7.78125
    And the pixel at 17, 20 of the window is "#000000"
    And the pixel at 18, 20 of the window is "#000000"
    And the pixel at 19, 20 of the window is "#ffffff"

  Scenario: A cursor on a pixel's edge covers that pixel only
    When the user presses "Home"
    And 50 milliseconds pass
    Then the property "cursorRectangle.x" of "input" is 0
    And the pixel at 9, 20 of the window is "#ffffff"
    And the pixel at 10, 20 of the window is "#000000"
