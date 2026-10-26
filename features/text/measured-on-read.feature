# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html (implicitWidth, implicitHeight)
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textfield.html#placeholderText-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Text.implicitWidth @api-QtQuick.Text.implicitHeight
Feature: A text's size is new as soon as what it follows is
  A script that gives a text another text or font and then asks how large
  it is gets the new size at once, without waiting for a frame. That holds
  for a text nobody sees: a hidden one, an empty one, and a field's
  placeholder, whose height the field takes.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property real hiddenWidth: -1; property real emptyHeight: -1
        property real fieldHeight: -1; property real shownWidth: -1
        function grow() {
          hidden.text = "a much longer text"; root.hiddenWidth = hidden.implicitWidth
          empty.font.pixelSize = 40; root.emptyHeight = empty.implicitHeight
          field.font.pixelSize = 40; root.fieldHeight = field.implicitHeight
          shown.text = "a much longer text"; root.shownWidth = shown.implicitWidth
        }
        Text { id: hidden; text: "hi"; visible: false }
        Text { id: empty }
        TextField { id: field }
        Text { id: shown; text: "hi"; width: 50; elide: Text.ElideRight }
        Text { id: wideRef; text: "a much longer text" }
        Text { id: emptyRef; font.pixelSize: 40 }
        TextField { id: fieldRef; font.pixelSize: 40 }
        property bool hiddenFresh: hiddenWidth === wideRef.implicitWidth
        property bool emptyFresh: emptyHeight === emptyRef.implicitHeight
        property bool fieldFresh: fieldHeight === fieldRef.implicitHeight
        property bool shownFresh: shownWidth === wideRef.implicitWidth
      }
      """
    When 50 milliseconds pass
    And the method "grow" of "root" is called

  Scenario: A hidden text's width is new as soon as its text is
    Then the property "hiddenFresh" of "root" is true

  Scenario: An empty text's height is new as soon as its font is
    Then the property "emptyFresh" of "root" is true

  Scenario: An empty field's height is new as soon as its font is
    Then the property "fieldFresh" of "root" is true

  # A shown text that is not empty is measured by the frame's polish; its
  # sizes reading new at once waits for the text polish to leave the graph
  # (the delegate track's dirty bits).
  @backlog
  Scenario: A shown text's width is new as soon as its text is
    Then the property "shownFresh" of "root" is true
