# Sources:
#   docs/ARCHITECTURE.md sections 6.2 and 19.4 (`app_objects`' `models`
#   and `model`)
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent finds every kind of model and the views that show it
  Beside a ListModel and the number or array a view is given, an
  application reads its rows from files: an XmlListModel parses a
  document, a FolderListModel lists a folder. Each is listed as a model
  with its roles and rows once read, and every model says where it is
  declared and which views show it, each view with where it is declared,
  so that a client can go from a table to the view that draws it.

  Background:
    Given the text file "feed.xml":
      """xml
      <rss><item><title>First</title></item><item><title>Second</title></item></rss>
      """
    And the text file "d/a.txt":
      """
      a
      """
    And the text file "d/b.txt":
      """
      b
      """
    And the QML:
      """qml
      import QtQuick
      import QtQml.XmlListModel
      import Qt.labs.folderlistmodel
      Item {
        XmlListModel {
          id: feed; objectName: "feed"; source: "feed.xml"; query: "/rss/item"
          XmlListModelRole { name: "title"; elementName: "title" }
        }
        FolderListModel { id: files; objectName: "files"; folder: "d" }
        ListView { objectName: "news"; width: 100; height: 60; model: feed; delegate: Text { text: title } }
        Repeater { objectName: "names"; model: files; Text { text: fileName } }
      }
      """
    And the inspector is connected

  Scenario: An XmlListModel is listed with its roles, rows and view
    When 500 milliseconds pass
    And the agent calls the tool "app_objects" with:
      """json
      {"model": "#feed"}
      """
    Then the answer's "/type" is "XmlListModel"
    And the answer's "/count" is 2
    And the answer's "/roles" is ["title"]
    And the answer's "/writable" is false
    And the answer's "/rows/1/values/title" is "Second"
    And the answer's "/at" matches "Scenario\.qml:5:[0-9]+$"
    And the answer's "/shown_by" has 1 entries
    And the answer's "/shown_by/0/name" is "ListView \"news\""
    And the answer's "/shown_by/0/at" matches "Scenario\.qml:10:[0-9]+$"

  Scenario: A FolderListModel is listed with a row a file
    When 500 milliseconds pass
    And the agent calls the tool "app_objects" with:
      """json
      {"model": "#files"}
      """
    Then the answer's "/type" is "FolderListModel"
    And the answer's "/count" is 2
    And the answer's "/writable" is false
    And the answer's "/rows/0/values/fileName" is "a.txt"
    And the answer's "/shown_by/0/name" is "Repeater \"names\""

  Scenario: The list names each model's place and views
    When 500 milliseconds pass
    And the agent calls the tool "app_objects" with:
      """json
      {"models": true}
      """
    Then the answer's text contains "XmlListModel \"feed\" 2 rows, roles title, shown by ListView \"news\""
    And the answer's text contains "FolderListModel \"files\" 2 rows"
    And the answer's text contains "shown by Repeater \"names\""
    And the answer's "/models/1/at" matches "Scenario\.qml:9:[0-9]+$"
    And the answer's "/models/1/shown_by/0/at" matches "Scenario\.qml:11:[0-9]+$"
