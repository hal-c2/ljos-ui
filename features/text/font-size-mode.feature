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
  23 px high fits in 22.5. Styled text (`<b>` and the like under
  `AutoText` or `StyledText`) fits as plain text does; rich text
  (`Text.RichText`) keeps its size, as on qml6.

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

  Rule: Within the width the text is drawn at the largest size that fits

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string words: "Hello world"
          Text { id: whole; text: root.words; font.pixelSize: 40 }
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
          property bool narrowFits: narrow.contentWidth <= narrow.width
            && narrow.contentWidth > narrow.width * 0.8
          property bool narrowKeepsItsFont: narrow.font.pixelSize === 40
          property bool narrowKeepsItsSize: narrow.implicitWidth === whole.implicitWidth
            && narrow.implicitHeight === whole.implicitHeight
          property bool roomyIsWhole: roomy.contentWidth === whole.contentWidth
          property bool fixedIsWhole: fixed.contentWidth === whole.contentWidth
        }
        """

    Scenario: Within the width
      Then the property "narrowFits" of "root" is true
      And the property "narrowKeepsItsFont" of "root" is true
      And the property "narrowKeepsItsSize" of "root" is true

    Scenario: Text that fits already keeps its size
      Then the property "roomyIsWhole" of "root" is true
      And the property "fixedIsWhole" of "root" is true

  Rule: Within the height the text is drawn at the largest size that fits

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string words: "Hello world"
          Text { id: whole; text: root.words; font.pixelSize: 40 }
          Text {
            id: low; height: whole.implicitHeight / 2
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.VerticalFit
          }
          Text {
            id: box; width: whole.implicitWidth; height: whole.implicitHeight / 2
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.Fit
          }
          property bool lowFits: low.contentHeight <= Math.ceil(low.height)
            && low.contentHeight > low.height * 0.7
          property bool lowIsNarrower: low.contentWidth < whole.contentWidth
          property bool boxFits: box.contentHeight <= Math.ceil(box.height)
            && box.contentWidth <= box.width
        }
        """

    Scenario: Within the height, and within both
      Then the property "lowFits" of "root" is true
      And the property "lowIsNarrower" of "root" is true
      And the property "boxFits" of "root" is true

  Rule: The text is not drawn below the smallest size

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string words: "Hello world"
          Text { id: whole; text: root.words; font.pixelSize: 40 }
          Text { id: twelve; text: root.words; font.pixelSize: 12 }
          Text {
            id: floor; width: 10
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.HorizontalFit
          }
          Text { id: twelvePoints; text: root.words; font.pointSize: 12 }
          Text {
            id: pointFloor; width: 10
            text: root.words; font.pointSize: 30; fontSizeMode: Text.HorizontalFit
          }
          property bool floorStops: floor.contentHeight === twelve.contentHeight
          property bool pointFloorStops: pointFloor.contentHeight === twelvePoints.contentHeight
        }
        """

    Scenario: Not below the smallest size
      Then the property "floorStops" of "root" is true
      And the property "pointFloorStops" of "root" is true

  Rule: Styled text, a text style and a Label fit the same way

    Scenario: Styled text shrinks to fit, and rich text keeps its size
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string words: "Hello <b>bold</b> world"
          Text { id: whole; text: root.words; font.pixelSize: 40 }
          Text { id: wholeRich; text: root.words; font.pixelSize: 40; textFormat: Text.RichText }
          Text {
            id: styled; width: whole.implicitWidth / 2
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.HorizontalFit
          }
          Text {
            id: rich; width: whole.implicitWidth / 2; textFormat: Text.RichText
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.HorizontalFit
          }
          Text {
            id: low; height: whole.implicitHeight / 2
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.VerticalFit
          }
          property bool styledFits: styled.contentWidth <= styled.width
            && styled.contentWidth > styled.width * 0.8
          property bool richIsWhole: rich.contentWidth === wholeRich.contentWidth
          property bool lowFits: low.contentHeight <= Math.ceil(low.height)
            && low.contentHeight > low.height * 0.7
          property bool keepsItsSize: styled.implicitWidth === whole.implicitWidth
            && styled.implicitHeight === whole.implicitHeight
        }
        """
      Then the property "styledFits" of "root" is true
      And the property "richIsWhole" of "root" is true
      And the property "lowFits" of "root" is true
      And the property "keepsItsSize" of "root" is true

    Scenario: Raised text shrinks as plain text does
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property string words: "Hello world"
          Text { id: whole; text: root.words; font.pixelSize: 40 }
          Text {
            id: plain; width: whole.implicitWidth / 2
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.Fit
          }
          Text {
            id: raised; width: whole.implicitWidth / 2; style: Text.Raised; styleColor: "gray"
            text: root.words; font.pixelSize: 40; fontSizeMode: Text.Fit
          }
          property bool same: raised.contentWidth === plain.contentWidth
            && raised.contentHeight === plain.contentHeight
          property bool fits: raised.contentWidth <= raised.width
        }
        """
      Then the property "same" of "root" is true
      And the property "fits" of "root" is true

    Scenario: A Label fits inside its padding
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root; objectName: "root"
          width: 400; height: 300
          property string words: "Tap, click or shake to throw dice"
          Text { id: whole; text: root.words; font.pixelSize: 64; font.bold: true }
          Label {
            id: label; anchors.fill: parent; padding: parent.width * 0.1
            horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
            fontSizeMode: Text.Fit; font.bold: true; font.pixelSize: 64
            text: root.words; wrapMode: Text.NoWrap; minimumPixelSize: 10
          }
          property bool shrinks: label.contentWidth < whole.contentWidth
          property real inside: label.width - label.leftPadding - label.rightPadding
          property bool fits: label.contentWidth <= inside && label.contentWidth > inside * 0.8
          property bool keepsItsFont: label.font.pixelSize === 64
        }
        """
      Then the property "shrinks" of "root" is true
      And the property "fits" of "root" is true
      And the property "keepsItsFont" of "root" is true
