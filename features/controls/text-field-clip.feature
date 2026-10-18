# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-textfield.html
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#autoScroll-prop
# Verified on Qt 6.11.2 (qml6, offscreen): a field draws its text only
# inside its padding, though its clip is false; a TextInput on its own
# draws past its edges.

@qt-parity @api-QtQuick.Controls.TextField
Feature: A field's text stays inside it
  A text longer than its field scrolls, and what scrolled out of the field
  is not drawn.

  # Ljós clips an item's content and its children together, and a field's
  # background is a child that lies under its padding: the text needs a
  # clip of its own in the renderer.
  @backlog
  Scenario: The part of a long text that scrolled out of the field is not drawn
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 300; height: 60
        color: "#ffffff"
        TextField {
          id: field
          x: 150; y: 10; width: 100
          font.pixelSize: 20
          persistentSelection: true
          selectionColor: "#0000ff"
          background: null
          text: "a text much longer than the field it is in"
          Component.onCompleted: selectAll()
        }
      }
      """
    Then the pixel at 200, 25 of the window is within 180 of "#0000ff"
    And the pixel at 100, 25 of the window is within 10 of "#ffffff"
