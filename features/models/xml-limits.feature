# Sources:
#   docs/ARCHITECTURE.md (bounded messages and off-thread parsing)
#   https://doc.qt.io/qt-6/qml-qtqml-xmllistmodel-xmllistmodel.html

@ljos-only
Feature: XML document resource limits
  Hostile XML cannot consume unbounded model memory or block the Engine.

  Scenario: A remote document exceeding the byte budget fails
    Given the web server streams 16777217 bytes at "feed.xml"
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        XmlListModel { objectName: "m"; source: "$SERVER/feed.xml"; query: "/a" }
      }
      """
    When 1 second passes
    Then the property "status" of "m" is 3
    And the property "count" of "m" is 0

  Scenario: Unknown entities stop reading without custom entity expansion
    Given the text file "feed.xml":
      """xml
      <a><b>safe</b><b>&custom;</b><b>unread</b></a>
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        XmlListModel {
          objectName: "m"; source: "feed.xml"; query: "/a/b"
          XmlListModelRole { name: "value" }
        }
      }
      """
    When 500 milliseconds pass
    Then the property "status" of "m" is 1
    And the property "count" of "m" is 2

  Scenario: Excessive nesting fails even when no rows match
    Given the text file "feed.xml":
      """xml
      <a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a><a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a></a>
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      Item {
        XmlListModel { objectName: "m"; source: "feed.xml"; query: "/none" }
      }
      """
    When 500 milliseconds pass
    Then the property "status" of "m" is 3
    And the property "count" of "m" is 0
