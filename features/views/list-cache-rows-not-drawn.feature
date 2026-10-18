# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (cacheBuffer, displayMarginBeginning, displayMarginEnd)
# Verified on Qt 6.11.2 (qml6, offscreen): rows made for the cacheBuffer
# read visible but are not drawn while wholly outside the view and its
# display margins, even though the view does not clip; a row partly
# inside, or touching their edge, is drawn whole.

@qt-parity @api-QtQuick.ListView.cacheBuffer
Feature: Rows a ListView makes for its cacheBuffer are not drawn
  A view makes rows past what it shows so that they are ready when they
  come in. Until they do, they are not drawn, so a view that does not
  clip leaves the window around it as it was; only rows inside the view,
  or inside its display margins, are.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"; width: 200; height: 400; color: "white"
        ListView {
          objectName: "list"
          y: 100; width: 200; height: 100; cacheBuffer: 200
          model: 20
          delegate: Rectangle {
            required property int index
            objectName: "r" + index; width: 200; height: 30
            color: index % 2 ? "#0000ff" : "#ff0000"
          }
        }
      }
      """

  Scenario: Rows after the view are made but not drawn
    When 200 milliseconds pass
    Then the pixel at 100, 105 of the window is "#ff0000"
    And the pixel at 100, 215 of the window is "#0000ff"
    And the pixel at 100, 225 of the window is "#ffffff"
    And the pixel at 100, 245 of the window is "#ffffff"
    And the property "visible" of "r4" is true

  Scenario: Rows scrolled before the view are not drawn
    When the property "contentY" of "list" is set to 100
    And 200 milliseconds pass
    Then the pixel at 100, 95 of the window is "#0000ff"
    And the pixel at 100, 85 of the window is "#ffffff"
    And the pixel at 100, 65 of the window is "#ffffff"
    And the property "visible" of "r1" is true

  Scenario: A row scrolled back into the view is drawn again
    When the property "contentY" of "list" is set to 100
    And 200 milliseconds pass
    And the property "contentY" of "list" is set to 0
    And 200 milliseconds pass
    Then the pixel at 100, 105 of the window is "#ff0000"
    And the pixel at 100, 225 of the window is "#ffffff"

  Scenario: Rows inside the display margin are drawn, up to its edge
    # The margin ends at 250, where the sixth row starts: it touches the
    # margin, so it is drawn.
    When the property "displayMarginEnd" of "list" is set to 50
    And 200 milliseconds pass
    Then the pixel at 100, 225 of the window is "#ff0000"
    And the pixel at 100, 255 of the window is "#0000ff"
    And the pixel at 100, 285 of the window is "#ffffff"
