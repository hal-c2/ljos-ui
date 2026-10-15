# Sources:
#   https://url.spec.whatwg.org/#api
#   https://doc.qt.io/qt-6/qtqml-javascript-hostenvironment.html
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: The JS URL global
  `new URL(text)` parses an absolute URL (or one relative to a base) into
  its parts, and `searchParams` reads and edits its query.

  @qt-parity
  Scenario Outline: The parts of an absolute URL
    Given the QML:
      """qml
      import QtQuick
      QtObject {
        objectName: "root"
        readonly property var u: new URL("https://user:pw@example.com:8080/a/b.mp4?x=1&y=two#frag")
        readonly property string got: String(u.<member>)
      }
      """
    Then the property "got" of "root" is "<expected>"

    Examples:
      | member   | expected                                                         |
      | href     | https://user:pw@example.com:8080/a/b.mp4?x=1&y=two#frag           |
      | protocol | https:                                                           |
      | username | user                                                             |
      | password | pw                                                               |
      | host     | example.com:8080                                                 |
      | hostname | example.com                                                      |
      | port     | 8080                                                             |
      | pathname | /a/b.mp4                                                         |
      | search   | ?x=1&y=two                                                       |
      | hash     | #frag                                                            |
      | origin   | https://example.com:8080                                         |

  # Qt's URL is built on QUrl: it keeps the default port, and gives "#" for
  # the hash of a URL that has none.
  @qt-diverges
  Scenario: Missing parts are empty strings and the default port is dropped
    Given the QML:
      """qml
      import QtQuick
      QtObject {
        objectName: "root"
        readonly property var u: new URL("http://example.com:80")
        readonly property string got: [u.port, u.pathname, u.search, u.hash, u.href].join("|")
      }
      """
    Then the property "got" of "root" is "|/|||http://example.com/"

  # Qt does not remove the dot segments of a relative reference.
  @qt-diverges
  Scenario: A relative URL is resolved against a base
    Given the QML:
      """qml
      import QtQuick
      QtObject {
        objectName: "root"
        readonly property string got: new URL("../c?q=1", "https://example.com/a/b/d").href
      }
      """
    Then the property "got" of "root" is "https://example.com/a/c?q=1"

  @qt-parity
  Scenario: Text that is no URL throws a TypeError
    Given the QML:
      """qml
      import QtQuick
      QtObject {
        objectName: "root"
        property string got
        Component.onCompleted: {
          try { new URL("not a url") } catch (e) { got = e.name }
        }
      }
      """
    Then the property "got" of "root" is "TypeError"

  @qt-parity
  Scenario: The search parameters are read
    Given the QML:
      """qml
      import QtQuick
      QtObject {
        objectName: "root"
        property string got
        Component.onCompleted: {
          var p = new URL("https://example.com/p?x=1&y=two&x=3").searchParams
          var seen = []
          p.forEach(function (value, key) { seen.push(key + "=" + value) })
          got = [p.get("x"), p.get("zz"), p.has("y"), p.has("zz"), seen.join(","),
                 p.toString()].join("|")
        }
      }
      """
    Then the property "got" of "root" is "1||true|false|x=1,y=two,x=3|x=1&y=two&x=3"

  # Qt writes a space as %20 and does not write a delete back to the URL.
  @qt-diverges
  Scenario: The search parameters are edited and the URL follows
    Given the QML:
      """qml
      import QtQuick
      QtObject {
        objectName: "root"
        property string got
        Component.onCompleted: {
          var u = new URL("https://example.com/p?a=1&b=2&a=3")
          var p = u.searchParams
          p.append("c", "x y")
          p.set("b", "9")
          p.delete("a")
          got = [p.toString(), u.search, u.href].join("|")
        }
      }
      """
    Then the property "got" of "root" is "b=9&c=x+y|?b=9&c=x+y|https://example.com/p?b=9&c=x+y"

  @qt-parity
  Scenario: Assigning parts changes the href
    Given the QML:
      """qml
      import QtQuick
      QtObject {
        objectName: "root"
        property string got
        Component.onCompleted: {
          var u = new URL("https://example.com/a")
          u.pathname = "/b c"
          u.hash = "h"
          u.search = "k=v"
          got = u.href + "|" + u.toString() + "|" + u.search + "|" + u.hash
        }
      }
      """
    Then the property "got" of "root" is "https://example.com/b%20c?k=v#h|https://example.com/b%20c?k=v#h|?k=v|#h"

  @qt-parity
  Scenario: A URL sets a url property
    Given the QML:
      """qml
      import QtQuick
      Image {
        objectName: "root"
        source: new URL("https://example.com/p.png")
      }
      """
    Then the property "source" of "root" is "https://example.com/p.png"
