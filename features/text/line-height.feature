# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#lineHeight-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#lineHeightMode-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Line height
  lineHeight scales the distance between lines (ProportionalHeight, the
  default) or sets it in pixels (FixedHeight).

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool half: loose.height === 1.5 * plain.height
        property bool tightStepIsHalf: tight.height - tightOne.height === 0.5 * one.height
        property bool tightKeepsLastLine: tightOne.height > 0.9 * one.height
        property bool smallFixedIsNatural: smallFixed.height === one.height
        property bool sameBaseline: fixed.baselineOffset === plain.baselineOffset
        Text { id: one; objectName: "one"; text: "a" }
        Text { id: plain; objectName: "plain"; text: "a\nb" }
        Text { id: loose; objectName: "loose"; text: "a\nb"; lineHeight: 1.5 }
        Text { id: tightOne; objectName: "tightOne"; text: "a"; lineHeight: 0.5 }
        Text { id: tight; objectName: "tight"; text: "a\nb"; lineHeight: 0.5 }
        Text { id: fixed; objectName: "fixed"; text: "a\nb"; lineHeight: 30; lineHeightMode: Text.FixedHeight }
        Text { id: fixedOne; objectName: "fixedOne"; text: "a"; lineHeight: 40; lineHeightMode: Text.FixedHeight }
        Text { id: smallFixed; objectName: "smallFixed"; text: "a"; lineHeight: 4; lineHeightMode: Text.FixedHeight }
        Text { id: none; objectName: "none"; lineHeight: 30; lineHeightMode: Text.FixedHeight }
      }
      """
    When 100 milliseconds pass

  Scenario: A proportional line height multiplies the height of every line
    Then the property "half" of "root" is true

  Scenario: A fixed line height is the height of every line in pixels
    Then the property "height" of "fixed" is 60
    And the property "height" of "fixedOne" is 40
    And the property "contentHeight" of "fixed" is 60

  Scenario: An empty Text is one fixed line tall
    Then the property "height" of "none" is 30

  Scenario: The last line is never shorter than its font
    Then the property "smallFixedIsNatural" of "root" is true
    And the property "tightKeepsLastLine" of "root" is true

  Scenario: A line height below one moves the lines closer together
    Then the property "tightStepIsHalf" of "root" is true

  Scenario: The extra space goes below each line
    Then the property "sameBaseline" of "root" is true

  Scenario: Changing the line height resizes the item
    When the property "lineHeight" of "fixed" is set to 50
    Then the property "height" of "fixed" is 100
