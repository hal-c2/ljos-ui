# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#linkActivated-signal
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#linkHovered-signal
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#hoveredLink-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#linkAt-method
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#linkColor-prop
# Verified on Qt 6.11.2 (qml6, offscreen). That a press beside the links
# goes to the item beneath is Qt 6.11.2's behaviour, found by trying.

@qt-parity
Feature: Links in a Text
  A link in styled, rich or Markdown text reports the address it was
  written with: when it is pressed and released, when the pointer is over
  it, and when QML asks what lies at a point. The address is only reported;
  nothing is opened or fetched.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string clicked: ""
        property string hovered: "nothing yet"
        property string found: "nothing yet"
        property int beneath: 0
        function probe(x, y) { found = links.linkAt(x, y) }
        function probeEnd() { found = links.linkAt(links.width - 2, 5) }
        function probeOther(item) { found = item.linkAt(2, 5) }
        MouseArea { anchors.fill: parent; onClicked: root.beneath += 1 }
        Text {
          id: links; objectName: "links"; x: 20; y: 20
          textFormat: Text.StyledText
          text: "<a href=\"one\">abc</a> def <a href=\"two\">ghi</a>"
          onLinkActivated: link => root.clicked = link
          onLinkHovered: link => root.hovered = link
        }
        Text { id: plain; y: 60; text: "abc def" }
        Text { id: rich; y: 100; textFormat: Text.RichText; text: "<a href=\"three\">abc</a>" }
        Text { id: md; y: 140; textFormat: Text.MarkdownText; text: "[abc](four) def" }
        function probePlain() { probeOther(plain) }
        function probeRich() { probeOther(rich) }
        function probeMarkdown() { probeOther(md) }
      }
      """
    When 100 milliseconds pass

  Scenario: A link is blue until linkColor says otherwise
    Then the property "linkColor" of "links" is "#0000ff"

  Scenario: linkAt gives the address of the link at a point
    When the method "probe" of "root" is called with 2, 5
    Then the property "found" of "root" is "one"
    When the method "probeEnd" of "root" is called
    Then the property "found" of "root" is "two"

  Scenario: linkAt gives nothing where there is no link
    When the method "probe" of "root" is called with 250, 5
    Then the property "found" of "root" is ""
    When the method "probe" of "root" is called with 2, 150
    Then the property "found" of "root" is ""

  Scenario: Plain text has no links
    When the method "probePlain" of "root" is called
    Then the property "found" of "root" is ""

  Scenario: Rich text and Markdown have links too
    When the method "probeRich" of "root" is called
    Then the property "found" of "root" is "three"
    When the method "probeMarkdown" of "root" is called
    Then the property "found" of "root" is "four"

  Scenario: Pressing and releasing a link activates it
    When the user presses the pointer at 23, 28
    And the user releases the pointer at 23, 28
    Then the property "clicked" of "root" is "one"
    And the property "beneath" of "root" is 0

  Scenario: A press beside the links goes to what lies beneath
    When the user presses the pointer at 150, 28
    And the user releases the pointer at 150, 28
    Then the property "clicked" of "root" is ""
    And the property "beneath" of "root" is 1

  Scenario: A press that leaves the link does not activate it
    When the user presses the pointer at 23, 28
    And the user releases the pointer at 150, 150
    Then the property "clicked" of "root" is ""

  Scenario: The link under the pointer is reported
    When the user moves the pointer to 23, 28
    Then the property "hoveredLink" of "links" is "one"
    And the property "hovered" of "root" is "one"

  Scenario: Leaving the link reports an empty address
    When the user moves the pointer to 23, 28
    And the user moves the pointer to 150, 150
    Then the property "hoveredLink" of "links" is ""
    And the property "hovered" of "root" is ""
