# Sources:
#   https://doc.qt.io/qt-6/qml-xmlhttprequest.html
#   https://doc.qt.io/qt-6/qtqml-javascript-qmlglobalobject.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The document of an XMLHttpRequest's reply
  A reply of an XML type is a document in `responseXML`, made of the
  read-only nodes of the W3C DOM: its elements, attributes, text and CDATA
  sections, with the white space between elements, and no comments. A
  reply that is not XML, or not well-formed, has none.

  Background:
    Given the web server serves the text file "feed.xml":
      """xml
      <?xml version="1.0" encoding="UTF-8"?>
      <!-- a comment -->
      <feed lang="en" n='2'>
        <title>News &amp; more</title>
        <item id="a"><![CDATA[x<y]]></item>
        <item id="b">Two<!-- c --></item>
        <empty/>
      </feed>
      """
    And the file "Fetch.qml":
      """qml
      import QtQuick
      QtObject {
        property string name
        property var doc: undefined
        property string type: ""
        property var http: new XMLHttpRequest()
        Component.onCompleted: {
          http.onload = () => { type = http.getResponseHeader("Content-Type"); doc = http.responseXML }
          http.open("GET", "$SERVER/" + name)
          http.send()
        }
      }
      """

  @api-QtQml.XMLHttpRequest.responseXML
  Scenario: The document tells its declaration and its root element
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Fetch { id: f; name: "feed.xml" }
        property string got: f.doc === undefined ? "" : [f.doc.nodeType, f.doc.nodeName,
          f.doc.xmlVersion, f.doc.xmlEncoding, f.doc.xmlStandalone,
          f.doc.documentElement.nodeName, f.doc.documentElement.parentNode,
          f.doc.childNodes.length, f.http.responseXML === f.doc].join(" ")
      }
      """
    When 1 second passes
    Then the property "got" of "root" is "9 #document 1.0 UTF-8 false feed  0 true"

  @api-QtQml.XMLHttpRequest.responseXML
  Scenario: An element's children are its elements, its text and the white space between them
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Fetch { id: f; name: "feed.xml" }
        function kinds(n) {
          let out = []
          for (let i = 0; i < n.childNodes.length; i++) {
            let c = n.childNodes[i]
            out.push(c.nodeType === 1 ? c.tagName : c.nodeType + (c.isElementContentWhitespace ? "ws" : "[" + c.data + "]"))
          }
          return out.join(",")
        }
        property string got: f.doc === undefined ? "" : kinds(f.doc.documentElement)
          + " | " + kinds(f.doc.documentElement.childNodes[1])
          + " | " + kinds(f.doc.documentElement.childNodes[3])
          + " | " + kinds(f.doc.documentElement.childNodes[5])
      }
      """
    When 1 second passes
    Then the property "got" of "root" is "3ws,title,3ws,item,3ws,item,3ws,empty,3ws | 3[News & more] | 4[x<y] | 3[Two]"

  @api-QtQml.XMLHttpRequest.responseXML
  Scenario: A node knows the nodes around it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Fetch { id: f; name: "feed.xml" }
        function name(n) { return n ? n.nodeName : "null" }
        property string got: {
          if (f.doc === undefined) return ""
          let feed = f.doc.documentElement, title = feed.childNodes[1]
          return [name(feed.firstChild), name(feed.lastChild), name(title.parentNode),
            name(title.previousSibling), name(title.nextSibling.nextSibling),
            name(feed.firstChild.previousSibling), name(feed.lastChild.nextSibling),
            title.firstChild.nodeValue, title.nodeValue, title.firstChild.length].join(" ")
        }
      }
      """
    When 1 second passes
    Then the property "got" of "root" is "#text #text feed #text item null null News & more  11"

  @api-QtQml.XMLHttpRequest.responseXML
  Scenario: An element's attributes are nodes that name it
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Fetch { id: f; name: "feed.xml" }
        property string got: {
          if (f.doc === undefined) return ""
          let a = f.doc.documentElement.attributes, out = [a.length]
          for (let i = 0; i < a.length; i++)
            out.push(a[i].nodeType + ":" + a[i].name + "=" + a[i].value + "/" + a[i].nodeName
              + "=" + a[i].nodeValue + "@" + a[i].ownerElement.tagName)
          return out.join(" ") + " " + f.doc.documentElement.firstChild.attributes
        }
      }
      """
    When 1 second passes
    Then the property "got" of "root" is "2 2:lang=en/lang=en@feed 2:n=2/n=2@feed null"

  @api-QtQml.XMLHttpRequest.responseXML
  Scenario: A node cannot be changed
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Fetch { id: f; name: "feed.xml" }
        property string got: ""
        Connections {
          target: f
          function onDocChanged() {
            let title = f.doc.documentElement.childNodes[1]
            title.firstChild.data = "changed"
            title.firstChild.nodeValue = "changed"
            root.got = title.firstChild.data
          }
        }
        id: root
      }
      """
    When 1 second passes
    Then the property "got" of "root" is "News & more"

  @api-QtQml.XMLHttpRequest.responseXML
  Scenario Outline: A reply that is not an XML document has none
    Given the web server serves the text file "<file>":
      """
      <text>
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Fetch { id: f; name: "<file>" }
        property string got: f.doc === undefined ? "" : f.type + " " + f.doc
      }
      """
    When 1 second passes
    Then the property "got" of "root" is "<got>"

    Examples:
      | file       | text       | got                   |
      | plain.txt  | <a>1</a>   | text/plain null       |
      | broken.xml | <a><b></a> | text/xml null         |
      | act.json   | {"n": 1}   | application/json null |

  @api-QtQml.XMLHttpRequest.responseXML
  Scenario: A request that has no reply yet has no document
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string got: ""
        Component.onCompleted: {
          let http = new XMLHttpRequest()
          got += http.responseXML
          http.onreadystatechange = () => {
            if (http.readyState === XMLHttpRequest.HEADERS_RECEIVED) got += " " + http.responseXML
          }
          http.open("GET", "$SERVER/feed.xml")
          http.send()
        }
      }
      """
    When 1 second passes
    Then the property "got" of "root" is "null null"
