# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#resolvedUrl-method
#   https://doc.qt.io/qt-6/qurl.html#resolved (RFC 3986, section 5.2)
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Resolving an address against the document
  Qt.resolvedUrl resolves a relative address against the address of the
  file the call is written in, as RFC 3986 does: dot segments are removed
  and an address that names a directory keeps its trailing slash.

  Rule: An address naming a directory ends in a slash

    Scenario Outline: "<relative>" resolves to the directory "<expected>"
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          readonly property string here: String(Qt.resolvedUrl("x")).replace(/x$/, "")
          // A directory that lost its slash would not start with `here`.
          property string got: String(Qt.resolvedUrl("<relative>")).replace(here, "@/")
        }
        """
      Then the property "got" of "root" is "<expected>"

      Examples:
        | relative | expected |
        | sub/     | @/sub/   |
        | .        | @/       |
        | ./       | @/       |
        | sub/..   | @/       |
        | sub/../  | @/       |
        | sub/./   | @/sub/   |
        | sub/.    | @/sub/   |

  Rule: A file stays a file

    Scenario Outline: "<relative>" resolves to "<expected>" beside the document
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          readonly property string here: String(Qt.resolvedUrl("x")).replace(/x$/, "")
          property string got: String(Qt.resolvedUrl("<relative>")).replace(here, "@/")
        }
        """
      Then the property "got" of "root" is "<expected>"

      Examples:
        | relative     | expected    |
        | sub/a.qml    | @/sub/a.qml |
        | sub/../a.qml | @/a.qml     |
        | ./a.qml      | @/a.qml     |
