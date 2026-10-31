# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-xmllistmodel-xmllistmodel.html
#   https://doc.qt.io/qt-6/qml-qtqml-xmllistmodel-xmllistmodelrole.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: XmlListModel
  An XmlListModel makes a read-only model of an XML document: the query
  picks the elements that become rows, and each XmlListModelRole reads a
  value out of a row's element. The document is read in the background;
  the model says Loading, then Ready when its rows are there.

  @api-QtQml.XmlListModel.XmlListModel.status
  Scenario Outline: XmlListModel.<name> is <value>
    Given the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item { objectName: "o"; property var v: XmlListModel.<name> }
      """
    Then the property "v" of "o" is <value>

    Examples:
      | name    | value |
      | Null    | 0     |
      | Ready   | 1     |
      | Loading | 2     |
      | Error   | 3     |

  @api-QtQml.XmlListModel.XmlListModel.count
  @api-QtQml.XmlListModel.XmlListModel.roles
  Scenario: Elements picked by the query become rows, roles read their values
    Given the text file "feed.xml":
      """xml
      <rss><channel>
      <item id="a1"><title made="2026-01-01">First</title><info><pages>10</pages></info></item>
      <item id="a2"><title made="2026-01-02">Second</title><info><pages>20</pages></info></item>
      </channel></rss>
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        id: root; objectName: "o"
        property string seen: ""
        property string rows: ""
        property int roleCount: m.roles.length
        XmlListModel {
          id: m; objectName: "m"; source: "feed.xml"; query: "/rss/channel/item"
          XmlListModelRole { name: "title"; elementName: "title" }
          XmlListModelRole { name: "stamp"; elementName: "title"; attributeName: "made" }
          XmlListModelRole { name: "pages"; elementName: "info/pages" }
          XmlListModelRole { name: "itemId"; elementName: ""; attributeName: "id" }
          onStatusChanged: root.seen += status + ";"
        }
        Repeater {
          model: m
          delegate: Text {
            required property string title; required property string stamp
            required property string pages; required property string itemId
            text: title + "|" + stamp + "|" + pages + "|" + itemId
            Component.onCompleted: root.rows += text + ";"
          }
        }
      }
      """
    When 500 milliseconds pass
    Then the property "status" of "m" is 1
    And the property "count" of "m" is 2
    And the property "progress" of "m" is 1
    And the property "seen" of "o" is "2;1;"
    And the property "rows" of "o" is "First|2026-01-01|10|a1;Second|2026-01-02|20|a2;"
    And the property "roleCount" of "o" is 4

  @api-QtQml.XmlListModel.XmlListModel.source
  Scenario: A model with no source has loaded nothing
    Given the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        id: root; objectName: "o"
        property string seen: ""
        XmlListModel {
          id: m; objectName: "m"; query: "/rss/item"
          XmlListModelRole { name: "t"; elementName: "title" }
          onStatusChanged: root.seen += status + ";"
        }
      }
      """
    When 500 milliseconds pass
    Then the property "status" of "m" is 0
    And the property "count" of "m" is 0
    And the property "seen" of "o" is "2;0;"

  # A file Qt cannot open reads as an empty document: Ready, no rows.
  Scenario: A source file that is not there is an empty model, ready
    Given the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        XmlListModel {
          objectName: "m"; source: "missing.xml"; query: "/rss/item"
          XmlListModelRole { name: "t"; elementName: "title" }
        }
      }
      """
    When 500 milliseconds pass
    Then the property "status" of "m" is 1
    And the property "count" of "m" is 0

  @api-QtQml.XmlListModel.XmlListModel.errorString
  Scenario: A document that cannot be fetched is an error that says why
    Given the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        id: root; objectName: "o"
        property string err: "?"
        XmlListModel {
          id: m; objectName: "m"; source: "http://127.0.0.1:9/feed.xml"; query: "/rss/item"
          XmlListModelRole { name: "t"; elementName: "title" }
          onStatusChanged: if (status === XmlListModel.Error) root.err = m.errorString()
        }
      }
      """
    When 1 second passes
    Then the property "status" of "m" is 3
    And the property "count" of "m" is 0
    And the property "err" of "o" is not "?"
    And the property "err" of "o" is not ""

  @api-QtQml.XmlListModel.XmlListModel.query
  Scenario: A query that matches no element is ready and empty
    Given the text file "feed.xml":
      """xml
      <rss><channel><item><title>First</title></item></channel></rss>
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        XmlListModel {
          objectName: "m"; source: "feed.xml"; query: "/rss/nope"
          XmlListModelRole { name: "t"; elementName: "title" }
        }
      }
      """
    When 500 milliseconds pass
    Then the property "status" of "m" is 1
    And the property "count" of "m" is 0

  Scenario: A query must start with a slash
    Given the text file "feed.xml":
      """xml
      <rss><channel><item><title>First</title></item></channel></rss>
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        XmlListModel {
          objectName: "m"; source: "feed.xml"; query: "rss/channel/item"
          XmlListModelRole { name: "t"; elementName: "title" }
        }
      }
      """
    When 500 milliseconds pass
    Then the property "status" of "m" is 2
    And the property "count" of "m" is 0
    And the log has a warning matching "An XmlListModel query must start with"

  Scenario: A value an element does not have reads as empty text
    Given the text file "feed.xml":
      """xml
      <docs><doc><title>Only</title></doc></docs>
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        id: root; objectName: "o"
        property string rows: ""
        XmlListModel {
          id: m; objectName: "m"; source: "feed.xml"; query: "/docs/doc"
          XmlListModelRole { name: "title"; elementName: "title" }
          XmlListModelRole { name: "gone"; elementName: "nope" }
        }
        Repeater {
          model: m
          delegate: Text {
            required property string title
            required property string gone
            text: title + "|" + gone + "|"
            Component.onCompleted: root.rows += text + ";"
          }
        }
      }
      """
    When 500 milliseconds pass
    Then the property "count" of "m" is 1
    And the property "rows" of "o" is "Only||;"

  Scenario: Setting the source later loads that document
    Given the text file "feed.xml":
      """xml
      <docs><doc><title>One</title></doc><doc><title>Two</title></doc></docs>
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        XmlListModel {
          objectName: "m"; query: "/docs/doc"
          XmlListModelRole { name: "t"; elementName: "title" }
        }
      }
      """
    When the property "source" of "m" is set to "feed.xml"
    And 500 milliseconds pass
    Then the property "status" of "m" is 1
    And the property "count" of "m" is 2

  Scenario: A relative source in a component file is read beside that file
    Given the text file "feed.xml":
      """xml
      <docs><doc><title>One</title></doc><doc><title>Two</title></doc></docs>
      """
    And the text file "team/feed.xml":
      """xml
      <docs><doc><title>Three</title></doc></docs>
      """
    And the file "team/Feed.qml":
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        property alias model: m
        XmlListModel {
          id: m; source: "feed.xml"; query: "/docs/doc"
          XmlListModelRole { name: "t"; elementName: "title" }
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      import "team"
      Item {
        objectName: "o"
        Feed { id: f }
        property int count: f.model.count
      }
      """
    When 500 milliseconds pass
    Then the property "count" of "o" is 1

  Scenario: A relative source on the root of a component file is read beside the file that uses it
    Given the text file "feed.xml":
      """xml
      <docs><doc><title>One</title></doc><doc><title>Two</title></doc></docs>
      """
    And the text file "team/feed.xml":
      """xml
      <docs><doc><title>Three</title></doc></docs>
      """
    And the file "team/Feed.qml":
      """qml
      import QtQuick
      import QtQml.XmlListModel
      XmlListModel {
        property var model: this
        source: "feed.xml"; query: "/docs/doc"
        XmlListModelRole { name: "t"; elementName: "title" }
      }
      """
    And the QML:
      """qml
      import QtQuick
      import "team"
      Item {
        objectName: "o"
        Feed { id: f }
        property int count: f.model.count
      }
      """
    When 500 milliseconds pass
    Then the property "count" of "o" is 2

  Scenario: A new source replaces the rows of the old one
    Given the text file "one.xml":
      """xml
      <docs><doc><title>Alpha</title></doc><doc><title>Beta</title></doc></docs>
      """
    And the text file "two.xml":
      """xml
      <docs><doc><title>Gamma</title></doc></docs>
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        id: root; objectName: "o"
        property string row0: ""
        property string row1: ""
        XmlListModel {
          id: m; objectName: "m"; source: "one.xml"; query: "/docs/doc"
          XmlListModelRole { name: "t"; elementName: "title" }
          onStatusChanged: if (status === XmlListModel.Loading) { root.row0 = ""; root.row1 = "" }
        }
        Repeater {
          model: m
          delegate: Text {
            required property int index
            required property string t
            Component.onCompleted: {
              if (index === 0) root.row0 = t
              if (index === 1) root.row1 = t
            }
          }
        }
      }
      """
    When 500 milliseconds pass
    Then the property "count" of "m" is 2
    And the property "row0" of "o" is "Alpha"
    And the property "row1" of "o" is "Beta"
    When the property "source" of "m" is set to "two.xml"
    And 500 milliseconds pass
    Then the property "count" of "m" is 1
    And the property "row0" of "o" is "Gamma"
    And the property "row1" of "o" is ""

  @api-QtQml.XmlListModel.XmlListModel.reload
  Scenario: reload reads the document again
    Given the text file "feed.xml":
      """xml
      <docs><doc><title>One</title></doc></docs>
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        id: root; objectName: "o"
        property string seen: ""
        XmlListModel {
          objectName: "m"; source: "feed.xml"; query: "/docs/doc"
          XmlListModelRole { name: "t"; elementName: "title" }
          onStatusChanged: root.seen += status + ";"
        }
      }
      """
    When 500 milliseconds pass
    Then the property "seen" of "o" is "2;1;"
    When the method "reload" of "m" is called
    And 500 milliseconds pass
    Then the property "seen" of "o" is "2;1;2;1;"
    And the property "count" of "m" is 1

  # A view shows its first frame while the document is still read, so the
  # order in which the model says it has loaded is part of what a view sees.
  @api-QtQml.XmlListModel.XmlListModel.progress
  Scenario: A local document is in progress at once, then delegates, count and status say it loaded
    Given the text file "feed.xml":
      """xml
      <docs><doc><title>One</title></doc><doc><title>Two</title></doc></docs>
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        id: root; objectName: "o"
        property string seen: ""
        XmlListModel {
          id: m; objectName: "m"; source: "feed.xml"; query: "/docs/doc"
          XmlListModelRole { name: "t"; elementName: "title" }
          onCountChanged: root.seen += "count " + count + ";"
          onProgressChanged: root.seen += "progress " + progress + ";"
          onStatusChanged: root.seen += "status " + status + ";"
        }
        Repeater {
          model: m
          delegate: Item {
            required property string t
            Component.onCompleted: root.seen += t + ";"
          }
        }
      }
      """
    When 500 milliseconds pass
    Then the property "seen" of "o" is "progress 1;status 2;One;Two;count 2;status 1;"

  # The document is read as a stream: a row that was read before the
  # document broke stays in the model, and the model is ready.
  Scenario: A document that breaks part-way keeps the rows already read
    Given the text file "bad.xml":
      """xml
      <docs><doc><title>Kept</title></doc><doc><title>Unclosed
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        XmlListModel {
          objectName: "m"; source: "bad.xml"; query: "/docs/doc"
          XmlListModelRole { name: "t"; elementName: "title" }
        }
      }
      """
    When 500 milliseconds pass
    Then the property "status" of "m" is 1
    And the property "count" of "m" is 2
    And the log has a warning matching "Query error"

  @ljos-only
  Scenario: A document is still loading until time has passed
    Given the text file "feed.xml":
      """xml
      <docs><doc><title>One</title></doc></docs>
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        XmlListModel {
          objectName: "m"; source: "feed.xml"; query: "/docs/doc"
          XmlListModelRole { name: "t"; elementName: "title" }
        }
      }
      """
    Then the property "status" of "m" is 2
    And the property "count" of "m" is 0
