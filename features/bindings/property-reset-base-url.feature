# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#baseUrl-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#baseUrl-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The base URL of a text, and taking it back with undefined
  Text and TextEdit resolve relative addresses in rich text from baseUrl.
  Until it is given one it is the address of the file the item is written
  in. A given address is kept as written, an empty one counts as none,
  and assigning undefined takes it back.

  Rule: Without one it is the address of the document

    Scenario Outline: A <type> that was given none has the address of its file
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          <type> { id: t }
          property string at: String(t.baseUrl)
          property bool file: /\.qml$/.test(at)
          property bool beside: at.replace(/[^\/]*$/, "x") === String(Qt.resolvedUrl("x"))
        }
        """
      Then the property "file" of "root" is true
      And the property "beside" of "root" is true

      Examples:
        | type     |
        | Text     |
        | TextEdit |

    Scenario Outline: An empty address given to a <type> reads back as the document's
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          <type> { id: plain }
          <type> { id: t; baseUrl: "http://example.org/dir/"; onBaseUrlChanged: root.changes++ }
          id: root
          property int changes: 0
          property bool same: false
          function empty() { t.baseUrl = ""; same = String(t.baseUrl) === String(plain.baseUrl) }
        }
        """
      When the method "empty" of "root" is called
      Then the property "same" of "root" is true
      And the property "changes" of "root" is 1

      Examples:
        | type     |
        | Text     |
        | TextEdit |

  Rule: A given address is kept as written

    Scenario Outline: A relative address given to a <type> is not resolved
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          <type> { id: declared; baseUrl: "sub/" }
          <type> { id: later }
          property string seen
          function give() {
            later.baseUrl = "rel/"
            seen = declared.baseUrl + "|" + later.baseUrl
          }
        }
        """
      When the method "give" of "root" is called
      Then the property "seen" of "root" is "sub/|rel/"

      Examples:
        | type     |
        | Text     |
        | TextEdit |

  Rule: Assigning undefined takes it back

    Scenario Outline: A <type> has the address of its file again, and says so once
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          <type> { id: plain }
          <type> { id: t; baseUrl: "http://example.org/dir/"; onBaseUrlChanged: root.changes++ }
          property int changes: 0
          property string seen
          function back() {
            t.baseUrl = undefined
            seen += (String(t.baseUrl) === String(plain.baseUrl)) + "," + changes + ";"
          }
        }
        """
      When the method "back" of "root" is called
      And the method "back" of "root" is called
      Then the property "seen" of "root" is "true,1;true,1;"

      Examples:
        | type     |
        | Text     |
        | TextEdit |

    Scenario Outline: Taking back what a <type> was never given changes nothing
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          <type> { id: t; onBaseUrlChanged: root.changes++ }
          property int changes: 0
          property bool same: false
          function back() {
            var was = String(t.baseUrl)
            t.baseUrl = undefined
            t.baseUrl = was
            same = String(t.baseUrl) === was
          }
        }
        """
      When the method "back" of "root" is called
      Then the property "same" of "root" is true
      And the property "changes" of "root" is 0

      Examples:
        | type     |
        | Text     |
        | TextEdit |

    Scenario: A binding that yields undefined takes it back, and a value gives it again
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          property bool given: true
          Text { id: plain }
          Text { id: t; baseUrl: root.given ? "http://example.org/dir/" : undefined }
          property bool same: String(t.baseUrl) === String(plain.baseUrl)
          property string at: t.baseUrl
        }
        """
      Then the property "same" of "root" is false
      When the property "given" of "root" is set to false
      Then the property "same" of "root" is true
      When the property "given" of "root" is set to true
      Then the property "at" of "root" is "http://example.org/dir/"

  Rule: Only undefined takes it back

    Scenario: null is refused and the address stays
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Text { id: t; baseUrl: "http://example.org/dir/" }
          property string said
          function clear() {
            try { t.baseUrl = null } catch (e) { said = String(e) }
            said += "|" + t.baseUrl
          }
        }
        """
      When the method "clear" of "root" is called
      Then the property "said" of "root" is "Error: Cannot assign std::nullptr_t to QUrl|http://example.org/dir/"
