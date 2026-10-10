# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#contentWidth-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#contentHeight-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#implicitWidth-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#baselineOffset-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The size of a Text
  A Text without a size is as large as its text. contentWidth and
  contentHeight say how large the laid-out text is whatever the item's size.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property bool sizedByText: free.width === free.implicitWidth && free.height === free.implicitHeight
        property bool contentIsImplicit: free.contentWidth === free.implicitWidth
        property bool boxedKeepsContent: boxed.contentWidth === free.contentWidth
        property bool boxedKeepsImplicit: boxed.implicitWidth === free.implicitWidth
        property bool spacesCount: spaced.width > free.width
        property bool longerIsWider: longer.width > free.width
        property bool sameAsLonger: free.width === longer.width
        property bool baselineInside: free.baselineOffset > 0 && free.baselineOffset < free.height
        Text { id: free; objectName: "free"; text: "Hello world" }
        Text { id: boxed; objectName: "boxed"; width: 300; height: 80; text: "Hello world" }
        Text { id: spaced; objectName: "spaced"; text: "Hello world  " }
        Text { id: longer; objectName: "longer"; text: "Hello world, and more" }
      }
      """
    When 100 milliseconds pass

  Scenario: A Text without a size takes the size of its text
    Then the property "sizedByText" of "root" is true
    And the property "contentIsImplicit" of "root" is true
    And the property "width" of "free" is at least 20

  Scenario: A size that was set does not change the content size
    Then the property "width" of "boxed" is 300
    And the property "height" of "boxed" is 80
    And the property "boxedKeepsContent" of "root" is true
    And the property "boxedKeepsImplicit" of "root" is true

  Scenario: Spaces at the end count
    Then the property "spacesCount" of "root" is true

  Scenario: Longer text is wider
    Then the property "longerIsWider" of "root" is true

  Scenario: The item follows a change of text
    When the property "text" of "free" is set to "Hello world, and more"
    Then the property "sameAsLonger" of "root" is true

  Scenario: The baseline lies inside the line
    Then the property "baselineInside" of "root" is true
