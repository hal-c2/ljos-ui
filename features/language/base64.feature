# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#btoa-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#atob-method
# Verified on Qt 6.12.0 (qml6, offscreen).

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
        // An ArrayBuffer's bytes, each as the character of its code.
        function bytes(b) { return Array.from(new Uint8Array(b), (c) => String.fromCharCode(c)).join("") }
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
      | bytes(Qt.btoa(new ArrayBuffer(2)))                                  | AAA= |
      | Object.prototype.toString.call(Qt.btoa(new ArrayBuffer(2)))         | [object ArrayBuffer] |
      | bytes(Qt.btoa([1]))                                                 | AQ== |
      | bytes(Qt.atob(new Uint8Array([89, 81]).buffer))                     | a    |
      | Qt.btoa(123).byteLength                                             | 0    |
      | Qt.btoa(new Uint8Array([97, 98])).byteLength                        | 0    |
      | Qt.atob(new Uint8Array([89, 81])).byteLength                        | 0    |

    Examples: an array's items are bytes: a number below 256, or one character below U+0100
      | expression                     | text |
      | bytes(Qt.btoa(["a", 98, 1.9])) | YWIB |
      | bytes(Qt.btoa([255.9, NaN]))   | /wA= |
      | bytes(Qt.btoa(["é"]))          | 6Q== |
      | bytes(Qt.btoa([256]))          | throws Invalid character |
      | bytes(Qt.btoa([-0.5]))         | throws Invalid character |
      | bytes(Qt.btoa(["ab"]))         | throws Invalid character |
      | bytes(Qt.btoa(["€"]))          | throws Invalid character |
      | bytes(Qt.btoa([true]))         | throws Invalid character |

    Examples: bytes decode only when they are all of the alphabet and padded right
      | expression                                              | text                     |
      | bytes(Qt.atob(["Y", "W", "I", "="]))                    | ab                       |
      | bytes(Qt.atob([89, 87, 73]))                            | ab                       |
      | bytes(Qt.atob(["Y", "Q", "=", "="]))                    | a                        |
      | bytes(Qt.atob(["Y"]))                                   |                          |
      | bytes(Qt.atob(["Y", " ", "Q"]))                         | throws Invalid character |
      | bytes(Qt.atob(["Y", "Q", "="]))                         | throws Invalid character |
      | bytes(Qt.atob(["Y", "Q", "=", "=", "A"]))               | throws Invalid character |
      | bytes(Qt.atob(["Y", "Q", "=", "=", "="]))               | throws Invalid character |
      | bytes(Qt.atob([0]))                                     | throws Invalid character |
      | bytes(Qt.atob(new Uint8Array([89, 81, 61, 61]).buffer)) | a                        |
      | bytes(Qt.atob(new Uint8Array([89, 32, 81]).buffer))     | throws Invalid character |
      | bytes(Qt.atob(new Uint8Array([89, 81, 61]).buffer))     | throws Invalid character |

    Examples: the wrong number of arguments throws
      | expression                     | text |
      | Qt.btoa()                      | throws Unable to determine callable overload.  Candidates are: |
      | Qt.btoa("a", "b")              | throws Too many arguments |
      | Qt.atob("YQ==", 1)             | throws Too many arguments |
