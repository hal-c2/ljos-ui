# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html#atYBeginning-prop
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html#atYEnd-prop
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html#topMargin-prop
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html#bottomMargin-prop
#   https://doc.qt.io/qt-6/qml-qtquick-flickable.html#returnToBounds-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Flickable atYBeginning, atYEnd, margins, and returnToBounds when content shrinks
  When contentHeight is at most the Flickable height and margins are zero, both atYBeginning and
  atYEnd are true at contentY 0. topMargin and bottomMargin shift the beginning and end bounds
  to -topMargin and contentHeight - height + bottomMargin, and returnToBounds snaps an
  out-of-bounds contentY to those limits.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 100; height: 100
        property real cy: fl.contentY
        property bool beg: fl.atYBeginning
        property bool end: fl.atYEnd
        Flickable {
          id: fl; objectName: "fl"; width: 100; height: 100
          contentWidth: 100; contentHeight: 60
          rebound: Transition {}
        }
        function growAndScroll() { fl.contentHeight = 300; fl.contentY = 200 }
        function shrinkAndReturn() {
          fl.contentHeight = 300; fl.contentY = 200
          fl.contentHeight = 150; fl.returnToBounds()
        }
        function applyMargins() {
          fl.contentHeight = 300; fl.topMargin = 20; fl.bottomMargin = 30
          fl.contentY = -50; fl.returnToBounds()
        }
        function overscrollBottomWithMargins() {
          fl.contentHeight = 300; fl.topMargin = 20; fl.bottomMargin = 30
          fl.contentY = 260; fl.returnToBounds()
        }
      }
      """

  Scenario: Content shorter than the viewport is at both the beginning and the end until it grows
    Then the property "beg" of "root" is true
    And the property "end" of "root" is true
    When the method "growAndScroll" of "root" is called
    Then the property "beg" of "root" is false
    And the property "end" of "root" is true

  Scenario: Shrinking contentHeight while scrolled to the bottom and calling returnToBounds clamps contentY
    When the method "shrinkAndReturn" of "root" is called
    Then the property "cy" of "root" is 50
    And the property "end" of "root" is true

  Scenario: topMargin and bottomMargin shift the bounds that returnToBounds clamps to
    When the method "applyMargins" of "root" is called
    Then the property "cy" of "root" is -20
    And the property "beg" of "root" is true
    When the method "overscrollBottomWithMargins" of "root" is called
    Then the property "cy" of "root" is 230
    And the property "end" of "root" is true
