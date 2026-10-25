# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#fontSizeMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#minimumPixelSize-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#minimumPointSize-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Text whose font shrinks to fit its item
  A label given a width or a height and `fontSizeMode` is drawn at the
  largest size, up to its font's, that fits: within the width on one line
  (`HorizontalFit`), within the height (`VerticalFit`) or within both
  (`Fit`). It does not shrink below `minimumPixelSize` (or
  `minimumPointSize`, for a font sized in points); what still does not
  fit is cut as `elide` says. The font itself keeps the size it was
  given, and so does the implicit size: it is the text's at that font.
  A height is fitted to the whole pixel above it, as on qml6 a 20 px line
  23 px high fits in 22.5.

  Scenario: The modes and the smallest sizes Qt starts with
    Given the QML:
      """qml
      import QtQuick
      Text {
        objectName: "label"
        property var modes: [Text.FixedSize, Text.HorizontalFit, Text.VerticalFit, Text.Fit]
      }
      """
    Then the property "fontSizeMode" of "label" is 0
    And the property "modes" of "label" is [0, 1, 2, 3]
    And the property "minimumPixelSize" of "label" is 12
    And the property "minimumPointSize" of "label" is 12

  Rule: The text is drawn at the largest size that fits

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string words: "Hello world"
          Text { id: whole; text: root.words; font.pixelSize: 40 }
          Text { id: twelve; text: root.words; font.pixelSize: 12 }
          Text {
            id: narrow; width: whole.implicitWidth / 2
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.HorizontalFit
          }
          Text {
            id: roomy; width: whole.implicitWidth * 2
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.HorizontalFit
          }
          Text {
            id: fixed; width: whole.implicitWidth / 2
            text: root.words; font.pixelSize: 40
          }
          Text {
            id: low; height: whole.implicitHeight / 2
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.VerticalFit
          }
          Text {
            id: box; width: whole.implicitWidth; height: whole.implicitHeight / 2
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.Fit
          }
          Text {
            id: floor; width: 10
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.HorizontalFit
          }
          Text { id: twelvePoints; text: root.words; font.pointSize: 12 }
          Text {
            id: pointFloor; width: 10
            text: root.words; font.pointSize: 30; fontSizeMode: Text.HorizontalFit
          }
          property bool narrowFits: narrow.contentWidth <= narrow.width
            && narrow.contentWidth > narrow.width * 0.8
          property bool narrowKeepsItsFont: narrow.font.pixelSize === 40
          property bool narrowKeepsItsSize: narrow.implicitWidth === whole.implicitWidth
            && narrow.implicitHeight === whole.implicitHeight
          property bool roomyIsWhole: roomy.contentWidth === whole.contentWidth
          property bool fixedIsWhole: fixed.contentWidth === whole.contentWidth
          property bool lowFits: low.contentHeight <= Math.ceil(low.height)
            && low.contentHeight > low.height * 0.7
          property bool lowIsNarrower: low.contentWidth < whole.contentWidth
          property bool boxFits: box.contentHeight <= Math.ceil(box.height)
            && box.contentWidth <= box.width
          property bool floorStops: floor.contentHeight === twelve.contentHeight
          property bool pointFloorStops: pointFloor.contentHeight === twelvePoints.contentHeight
        }
        """

    Scenario: Within the width
      Then the property "narrowFits" of "root" is true
      And the property "narrowKeepsItsFont" of "root" is true
      And the property "narrowKeepsItsSize" of "root" is true

    Scenario: Text that fits already keeps its size
      Then the property "roomyIsWhole" of "root" is true
      And the property "fixedIsWhole" of "root" is true

    Scenario: Within the height, and within both
      Then the property "lowFits" of "root" is true
      And the property "lowIsNarrower" of "root" is true
      And the property "boxFits" of "root" is true

    Scenario: Not below the smallest size
      Then the property "floorStops" of "root" is true
      And the property "pointFloorStops" of "root" is true
