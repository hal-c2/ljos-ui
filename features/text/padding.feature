# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#padding-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#implicitWidth-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The padding of a Text
  A Text's padding goes around its words: it adds to the implicit size on
  each side. A side padding follows `padding` until it is given a value of
  its own. That holds whether or not anything reads the side paddings.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property real extraWidth: plain.implicitWidth - plain.contentWidth
        property real extraHeight: plain.implicitHeight - plain.contentHeight
        property real ownExtraWidth: own.implicitWidth - own.contentWidth
        Text { id: plain; objectName: "plain"; padding: 6; text: "Hello" }
        Text { id: own; objectName: "own"; padding: 6; leftPadding: 2; text: "Hello" }
      }
      """
    When 100 milliseconds pass

  Scenario: The padding goes on every side
    Then the property "extraWidth" of "root" is 12
    And the property "extraHeight" of "root" is 12

  Scenario: A change of the padding changes every side
    When the property "padding" of "plain" is set to 10
    Then the property "extraWidth" of "root" is 20
    And the property "extraHeight" of "root" is 20

  Scenario: A side padding reads the padding and follows it
    Then the property "leftPadding" of "plain" is 6
    When the property "padding" of "plain" is set to 3
    Then the property "leftPadding" of "plain" is 3
    And the property "bottomPadding" of "plain" is 3

  Scenario: A side padding of its own is kept
    Then the property "ownExtraWidth" of "root" is 8
    When the property "padding" of "own" is set to 1
    Then the property "ownExtraWidth" of "root" is 3

  Scenario: A side padding given later stops following the padding
    When the property "rightPadding" of "plain" is set to 0
    Then the property "extraWidth" of "root" is 6
    When the property "padding" of "plain" is set to 4
    Then the property "extraWidth" of "root" is 4
