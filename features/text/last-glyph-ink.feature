# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#implicitWidth-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#contentWidth-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textmetrics.html#advanceWidth-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#truncated-prop
# Verified on Qt 6.11.2 (qml6, offscreen): a line is as wide as its
# advances plus the whole pixels its last glyph's ink reaches past its own
# advance. TextMetrics' advanceWidth leaves that out, and eliding fits by
# advances only. Wrapping counts it: a line whose last glyph's ink would
# cross the width breaks before that glyph.

@qt-parity
Feature: The ink of a line's last glyph
  A Text is wide enough to show the whole of the last glyph on its line,
  even where that glyph's ink reaches past where the next glyph would start.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        TextMetrics { id: aw; font.pixelSize: 12; text: "aW" }
        TextMetrics { id: wa; font.pixelSize: 12; text: "Wa" }
        Text { id: endsW; objectName: "endsW"; font.pixelSize: 12; text: "aW" }
        Text { id: endsA; objectName: "endsA"; font.pixelSize: 12; text: "Wa" }
        Text {
          id: elided; objectName: "elided"; font.pixelSize: 12; text: "aW"
          width: 18.2; elide: Text.ElideRight
        }
        property real past: endsW.implicitWidth - aw.advanceWidth
        property bool pastIsWhole: past >= 1 && past === Math.round(past)
        property bool aEndsOnItsAdvance: endsA.implicitWidth === wa.advanceWidth
        property bool contentToo: endsW.contentWidth === endsW.implicitWidth
        property bool keepsItsText: elided.contentWidth === endsW.implicitWidth
      }
      """
    When 100 milliseconds pass

  Scenario: Ink past the last glyph's advance widens the line by whole pixels
    Then the property "pastIsWhole" of "root" is true
    And the property "contentToo" of "root" is true

  Scenario: A last glyph whose ink stays inside its advance adds nothing
    Then the property "aEndsOnItsAdvance" of "root" is true

  Scenario: Eliding fits the advances, not the ink
    Then the property "truncated" of "elided" is true
    And the property "keepsItsText" of "root" is true

  Rule: Wrapping keeps the last glyph's ink inside the width

    Scenario Outline: "aWaW" at <width> wraps into <lines> lines
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          TextMetrics { id: aw; font.pixelSize: 12; text: "aW" }
          Text { id: ink; font.pixelSize: 12; text: "aW" }
          Text {
            objectName: "wrapped"; font.pixelSize: 12; text: "aWaW"
            wrapMode: Text.WrapAnywhere; width: <width>
          }
        }
        """
      When 100 milliseconds pass
      Then the property "lineCount" of "wrapped" is <lines>

      Examples:
        | width                   | lines |
        | aw.advanceWidth + 0.2   | 3     |
        | ink.implicitWidth       | 2     |
