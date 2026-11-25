# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#btoa-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#atob-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Base64 with Qt.btoa and Qt.atob
  `Qt.btoa` codes text in base64, and `Qt.atob` gives the text back. Text
  is coded as its UTF-8; bytes in an ArrayBuffer come back as an
  ArrayBuffer.

  @api-QtQml.Qt.btoa @api-QtQml.Qt.atob
  Scenario Outline: <expression> is "<text>"
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string out: {
          try { return <expression> } catch (e) { return "throws " + e.message.split("\n")[0] }
        }
      }
      """
    Then the property "out" of "root" is "<text>"

    Examples: text is coded as its UTF-8
      | expression                     | text |
      | Qt.btoa("ab")                  | YWI= |
      | Qt.btoa("")                    |      |
      | Qt.btoa("é")                   | w6k= |
      | Qt.btoa("€")                   | 4oKs |
      | Qt.btoa("a" + String.fromCharCode(0xd800) + "b") | YWI= |
      | typeof Qt.btoa("a")            | string |

    Examples: decoding skips what is not of the alphabet
      | expression                     | text |
      | Qt.atob("YWI=")                | ab   |
      | Qt.atob("YWI")                 | ab   |
      | Qt.atob("YW I=")               | ab   |
      | Qt.atob("w6k=")                | é    |
      | Qt.atob("4oKs")                | €    |
      | Qt.atob("!!")                  |      |
      | Qt.atob("Y=Q")                 | a    |
      | Qt.atob("6Q==").charCodeAt(0)  | 65533 |

    Examples: bytes come back as bytes
      | expression                                                          | text |
      | String(Qt.btoa(new ArrayBuffer(2)))                                 | AAA= |
      | Object.prototype.toString.call(Qt.btoa(new ArrayBuffer(2)))         | [object ArrayBuffer] |
      | String(Qt.btoa([1]))                                                | AQ== |
      | String(Qt.atob(new Uint8Array([89, 81]).buffer))                    | a    |
      | Qt.btoa(123).byteLength                                             | 0    |

    Examples: the wrong number of arguments throws
      | expression                     | text |
      | Qt.btoa()                      | throws Unable to determine callable overload.  Candidates are: |
      | Qt.btoa("a", "b")              | throws Too many arguments |
      | Qt.atob("YQ==", 1)             | throws Too many arguments |
