# Sources:
#   https://doc.qt.io/qt-6/qml-xmlhttprequest.html
#   https://doc.qt.io/qt-6/qtqml-javascript-qmlglobalobject.html
#   https://xhr.spec.whatwg.org/
# Verified on Qt 6.11.2 (qmltestrunner and qml6, offscreen).
#
# Qt also says readyState 3 for each redirect that has a body; these
# scenarios do not follow one.

@qt-parity
Feature: XMLHttpRequest
  A script fetches from a web server, and sends to it, with the
  XMLHttpRequest every QML script sees. The reply comes in the background
  and readyState says how far it is; a status the server sends is an
  answer, while a server that cannot be reached is an error, with status 0.

  Background:
    Given the web server serves the text file "act.json":
      """
      {"activity": "Read a book", "n": 3}
      """

  Scenario: A script fetches JSON and reads it when it is done
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string states: ""
        property string got: ""
        Component.onCompleted: {
          let http = new XMLHttpRequest()
          http.onreadystatechange = function () {
            root.states += http.readyState
            if (http.readyState === XMLHttpRequest.DONE)
              root.got = http.status + " " + http.statusText + " "
                + JSON.parse(http.responseText.toString()).activity + " "
                + http.getResponseHeader("Content-Type")
          }
          http.open("GET", "$SERVER/act.json", true)
          http.setRequestHeader("Content-type", "application/json")
          http.setRequestHeader("Connection", "close")
          http.send()
          root.states += "/"
        }
      }
      """
    When 1 second passes
    Then the property "states" of "root" is "1/234"
    And the property "got" of "root" is "200 OK Read a book application/json"

  Scenario: The response is JSON or bytes when the script asks for them
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string json: ""
        property string bytes: ""
        function fetch(type, done) {
          let http = new XMLHttpRequest()
          http.responseType = type
          http.onload = function () { done(http.response) }
          http.open("GET", "$SERVER/act.json")
          http.send()
        }
        Component.onCompleted: {
          fetch("json", r => root.json = typeof r + " " + r.activity)
          fetch("arraybuffer", r => root.bytes = (r instanceof ArrayBuffer) + " " + r.byteLength)
        }
      }
      """
    When 1 second passes
    Then the property "json" of "root" is "object Read a book"
    And the property "bytes" of "root" is "true 35"

  Scenario: A name the server does not have is an answer with its status
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string got: ""
        Component.onCompleted: {
          let http = new XMLHttpRequest()
          http.onload = function () { root.got += "load " + http.status + " " + http.statusText }
          http.onerror = function () { root.got += "error" }
          http.onloadend = function () { root.got += " end" }
          http.open("GET", "$SERVER/missing.json")
          http.send()
        }
      }
      """
    When 1 second passes
    Then the property "got" of "root" is "load 404 Not Found end"

  Scenario: A server that cannot be reached is an error with status 0
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string states: ""
        property string got: ""
        Component.onCompleted: {
          let http = new XMLHttpRequest()
          http.onreadystatechange = function () { root.states += http.readyState }
          http.onload = function () { root.got += "load" }
          http.onerror = function () {
            root.got += "error " + http.status + " [" + http.statusText + "] ["
              + http.responseText + "] [" + http.getAllResponseHeaders() + "]"
          }
          http.onloadend = function () { root.got += " end" }
          http.open("GET", "http://127.0.0.1:9/nothing")
          http.send()
        }
      }
      """
    When 1 second passes
    Then the property "states" of "root" is "14"
    And the property "got" of "root" is "error 0 [] [] [] end"

  Scenario: A script sends a body with its own headers
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string got: ""
        Component.onCompleted: {
          let http = new XMLHttpRequest()
          http.onload = function () { root.got = http.status + "|" + http.responseText }
          http.open("POST", "$SERVER/echo")
          http.setRequestHeader("Content-Type", "application/json")
          http.setRequestHeader("X-One", "1")
          http.setRequestHeader("X-One", "2")
          http.send(JSON.stringify({ a: 1 }))
        }
      }
      """
    When 1 second passes
    Then the property "got" of "root" is "200|POST\ncontent-type: application/json;charset=UTF-8\nx-one: 1,2\n\n{\"a\":1}"

  Scenario: What is asked of a request before it is ready throws
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string got: ""
        function attempt(what, f) {
          try { root.got += what + ":" + f() + " " }
          catch (e) { root.got += what + ":" + e.code + " " + e.message + " " }
        }
        Component.onCompleted: {
          let http = new XMLHttpRequest()
          attempt("text", () => "[" + http.responseText + "]")
          attempt("header", () => http.setRequestHeader("a", "b"))
          attempt("send", () => http.send())
          attempt("status", () => http.status)
          attempt("method", () => http.open("TRACE", "$SERVER/act.json"))
          http.open("GET", "$SERVER/act.json")
          attempt("state", () => http.readyState)
          attempt("statusText", () => http.statusText)
          attempt("reply", () => http.getResponseHeader("content-type"))
        }
      }
      """
    Then the property "got" of "root" is "text:[] header:11 Invalid state send:11 Invalid state status:11 Invalid state method:12 Unsupported HTTP method type state:1 statusText:11 Invalid state reply:11 Invalid state "

  Scenario: A request stopped before its reply hears nothing more
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string states: ""
        Component.onCompleted: {
          let http = new XMLHttpRequest()
          http.onreadystatechange = function () { root.states += http.readyState }
          http.open("GET", "$SERVER/act.json")
          http.send()
          http.abort()
          root.states += "/" + http.readyState
        }
      }
      """
    When 1 second passes
    Then the property "states" of "root" is "14/0"

  Scenario: A request that is not asynchronous is done when send returns
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string got: ""
        Component.onCompleted: {
          let http = new XMLHttpRequest()
          http.open("GET", "$SERVER/act.json", false)
          http.send()
          root.got = http.readyState + " " + http.status + " " + JSON.parse(http.responseText).n
        }
      }
      """
    Then the property "got" of "root" is "4 200 3"

  @api-QtQml.XMLHttpRequest.readyState
  Scenario Outline: An error status says LOADING twice, or once when it has no body
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string got: ""
        Component.onCompleted: {
          let http = new XMLHttpRequest(), seen = []
          http.onreadystatechange = function () { seen.push(http.readyState) }
          http.onload = function () { seen.push("load " + http.status) }
          http.onerror = function () { seen.push("error") }
          http.onloadend = function () { root.got = seen.join(" ") }
          http.open("GET", "$SERVER/<path>")
          http.send()
        }
      }
      """
    When 1 second passes
    Then the property "got" of "root" is "<seen>"

    Examples:
      | path                 | seen                 |
      | act.json             | 1 2 3 4 load 200     |
      | missing.json         | 1 2 3 3 4 load 404   |
      | status/500/Broken    | 1 3 4 load 500       |
      | status/204/Nothing   | 1 2 3 4 load 204     |

  @api-QtQml.XMLHttpRequest.send
  Scenario: A request says it is a browser that reads the application's language, unless the script says otherwise
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string mine: ""
        property string own: ""
        // The locale's language and territory, English after another one, then any.
        readonly property string tag: Qt.locale().name.replace("_", "-")
        readonly property string accepted: (tag === "C" ? "" : tag + ",")
          + (tag.split("-")[0] === "en" ? "" : "en,") + "*"
        function fetch(headers, done) {
          let http = new XMLHttpRequest()
          http.onload = function () {
            const sent = http.responseText.split("\n")
            done(sent.find(l => l.startsWith("user-agent: ")).slice(12),
                 sent.find(l => l.startsWith("accept-language: ")).slice(17))
          }
          http.open("GET", "$SERVER/headers")
          for (const name in headers) http.setRequestHeader(name, headers[name])
          http.send()
        }
        Component.onCompleted: {
          fetch({}, (agent, language) => root.mine = agent + " " + (language === accepted))
          fetch({ "User-Agent": "Mine", "Accept-Language": "is" }, (a, l) => root.own = a + " " + l)
        }
      }
      """
    When 1 second passes
    Then the property "mine" of "root" is "Mozilla/5.0 true"
    And the property "own" of "root" is "Mine is"

  @api-QtQml.XMLHttpRequest.send
  Scenario: A body sent without a type is said to be text
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string text: ""
        property string bytes: ""
        function post(body, done) {
          let http = new XMLHttpRequest()
          http.onload = function () { done(http.responseText) }
          http.open("POST", "$SERVER/echo")
          http.send(body)
        }
        Component.onCompleted: {
          post("hi", r => root.text = r)
          post(new Uint8Array([65, 66]).buffer, r => root.bytes = r)
        }
      }
      """
    When 1 second passes
    Then the property "text" of "root" is "POST\ncontent-type: text/plain;charset=UTF-8\n\nhi"
    And the property "bytes" of "root" is "POST\ncontent-type: text/plain;charset=UTF-8\n\nAB"

  # The fetcher's HTTP client keeps the status and not the phrase the
  # server sent with it, so Ljós has the standard phrase.
  @backlog @api-QtQml.XMLHttpRequest.statusText
  Scenario: statusText is the reason phrase the server sent
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string got: ""
        Component.onCompleted: {
          let http = new XMLHttpRequest()
          http.onloadend = function () { root.got = http.status + " " + http.statusText }
          http.open("GET", "$SERVER/status/404/Gone%20Away")
          http.send()
        }
      }
      """
    When 1 second passes
    Then the property "got" of "root" is "404 Gone Away"
