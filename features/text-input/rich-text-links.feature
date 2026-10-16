# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#hoveredLink-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#linkActivated-signal
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#linkAt-method
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#positionToRectangle-method
# Verified on Qt 6.11.2 (qml6, offscreen; pointer input over its vnc platform).

@qt-parity
Feature: Links and images in an editable rich text
  A link in the rich text of a TextEdit tells when the pointer is over it
  and when it is clicked, while the text can still be edited. An image in
  the text is one character as wide as the image.

  Rule: Links

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 300; height: 200
          property string activated
          property string found: "none"
          function look(x, y) { found = edit.linkAt(x, y) }
          TextEdit {
            id: edit; objectName: "edit"; width: 280; height: 100
            textFormat: TextEdit.RichText
            text: "<a href=\"http://example.org\">link</a> after"
            onLinkActivated: (link) => root.activated = link
          }
        }
        """

    Scenario: The pointer over a link makes it the hovered link
      When the user moves the pointer to 8, 8
      Then the property "hoveredLink" of "edit" is "http://example.org"

    Scenario: The pointer away from the link leaves no hovered link
      When the user moves the pointer to 8, 8
      And the user moves the pointer to 250, 8
      Then the property "hoveredLink" of "edit" is ""

    Scenario: Clicking a link in an editable text activates it
      When the user presses the pointer at 8, 8
      And the user releases the pointer at 8, 8
      Then the property "activated" of "root" is "http://example.org"

    Scenario: Clicking a link in a read-only text activates it
      When the property "readOnly" of "edit" is set to true
      And the user presses the pointer at 8, 8
      And the user releases the pointer at 8, 8
      Then the property "activated" of "root" is "http://example.org"

    Scenario: Typing in a link keeps the link
      When the property "cursorPosition" of "edit" is set to 2
      And the property "focus" of "edit" is set to true
      And the user types "x"
      And the user moves the pointer to 8, 8
      Then the property "hoveredLink" of "edit" is "http://example.org"
      And the property "length" of "edit" is 11

    Scenario: linkAt names the link at a point
      When the method "look" of "root" is called with 8, 8
      Then the property "found" of "root" is "http://example.org"

    Scenario: linkAt is empty away from a link
      When the method "look" of "root" is called with 250, 8
      Then the property "found" of "root" is ""

  Rule: Images

    Background:
      Given a 30 by 12 image file "pic.png" filled with "#ff0000"
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 300; height: 200
          property real sized
          property real natural
          function measure() {
            sized = edit.positionToRectangle(2).x - edit.positionToRectangle(1).x
            natural = own.positionToRectangle(2).x - own.positionToRectangle(1).x
          }
          TextEdit {
            id: edit; objectName: "edit"; width: 280
            textFormat: TextEdit.RichText
            text: "a<img src=\"pic.png\" width=\"20\" height=\"10\">b"
          }
          TextEdit {
            id: own; objectName: "own"; y: 100; width: 280
            textFormat: TextEdit.RichText
            text: "a<img src=\"pic.png\">b"
          }
        }
        """
      When 100 milliseconds pass
      And the method "measure" of "root" is called

    Scenario: An image is one character
      Then the property "length" of "edit" is 3

    Scenario: An image is as wide as its width attribute says
      Then the property "sized" of "root" is 20

    Scenario: An image without a width is as wide as the picture
      Then the property "natural" of "root" is 30
