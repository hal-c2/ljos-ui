# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#horizontalAlignment-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#verticalAlignment-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#effectiveHorizontalAlignment-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#baselineOffset-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Text alignment and direction
  Text is aligned to the side its script starts from: left-to-right text to the
  left, right-to-left text to the right. Where the glyphs land is not visible
  through properties, so these scenarios read the alignment and the baseline.

  Rule: Horizontal alignment follows the direction of the text

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property bool latinIsLeft: latin.horizontalAlignment === Text.AlignLeft
          property bool hebrewIsRight: hebrew.horizontalAlignment === Text.AlignRight
          property bool mixedIsRight: mixed.horizontalAlignment === Text.AlignRight
          property bool endsHebrewIsLeft: endsHebrew.horizontalAlignment === Text.AlignLeft
          property bool forcedIsLeft: forced.horizontalAlignment === Text.AlignLeft
          property bool effectiveIsRight: hebrew.effectiveHorizontalAlignment === Text.AlignRight
          property bool hebrewHasWidth: hebrew.contentWidth > 20
          Text { id: latin; objectName: "latin"; width: 200; text: "Hello world" }
          Text { id: hebrew; objectName: "hebrew"; width: 200; text: "שלום עולם" }
          Text { id: mixed; objectName: "mixed"; width: 200; text: "שלום world" }
          Text { id: endsHebrew; objectName: "endsHebrew"; width: 200; text: "hello עולם" }
          Text {
            id: forced; objectName: "forced"; width: 200; text: "שלום עולם"
            horizontalAlignment: Text.AlignLeft
          }
        }
        """
      When 100 milliseconds pass

    Scenario: Left-to-right text is aligned left
      Then the property "latinIsLeft" of "root" is true

    Scenario: Right-to-left text is aligned right
      Then the property "hebrewIsRight" of "root" is true
      And the property "effectiveIsRight" of "root" is true
      And the property "hebrewHasWidth" of "root" is true

    Scenario: The first letter with a direction decides
      Then the property "mixedIsRight" of "root" is true
      And the property "endsHebrewIsLeft" of "root" is true

    Scenario: An alignment that was set is kept
      Then the property "forcedIsLeft" of "root" is true

    Scenario: The alignment follows a change of text
      When the property "text" of "latin" is set to "שלום"
      Then the property "latinIsLeft" of "root" is false
      When the property "text" of "latin" is set to "Hello"
      Then the property "latinIsLeft" of "root" is true

  Rule: Vertical alignment moves the baseline

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property real room: 100 - top.contentHeight
          property bool centred: centre.baselineOffset === top.baselineOffset + root.room / 2
          property bool atBottom: bottom.baselineOffset === top.baselineOffset + root.room
          property bool topIsNatural: top.baselineOffset === free.baselineOffset
          property bool sameContent: centre.contentHeight === free.height
          Text { id: free; objectName: "free"; text: "Hello" }
          Text { id: top; objectName: "top"; height: 100; text: "Hello" }
          Text {
            id: centre; objectName: "centre"; height: 100; text: "Hello"
            verticalAlignment: Text.AlignVCenter
          }
          Text {
            id: bottom; objectName: "bottom"; height: 100; text: "Hello"
            verticalAlignment: Text.AlignBottom
          }
        }
        """
      When 100 milliseconds pass

    Scenario: Text starts at the top
      Then the property "topIsNatural" of "root" is true

    Scenario: AlignVCenter puts half the free height above the text
      Then the property "centred" of "root" is true
      And the property "sameContent" of "root" is true

    Scenario: AlignBottom puts all the free height above the text
      Then the property "atBottom" of "root" is true

    Scenario: The baseline moves when the alignment changes
      When the property "verticalAlignment" of "top" is set to 64
      Then the property "atBottom" of "root" is false
      And the property "topIsNatural" of "root" is false
