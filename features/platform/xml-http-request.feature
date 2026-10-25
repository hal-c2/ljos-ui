# Sources:
#   https://doc.qt.io/qt-6/qml-xmlhttprequest.html
#   https://doc.qt.io/qt-6/qtqml-javascript-qmlglobalobject.html
#   https://xhr.spec.whatwg.org/
# Verified on Qt 6.11.2 (qmltestrunner and qml6, offscreen).
#
# Qt says readyState 3 once more for a reply with an error status (and
# once for each redirect it follows); these scenarios do not count it.

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
