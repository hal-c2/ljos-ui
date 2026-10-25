# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-droparea.html
#   https://doc.qt.io/qt-6/qml-qtquick-dragevent.html
# A drag from another application is made by the window system, which the
# oracle's driver cannot stand in for, so these scenarios run on Ljós only.
# The steps say what the window system tells the application; "taken" is
# what the application answers it.

@ljos-only
Feature: A drag from another application
  Another application may drag files, links, text or data of any MIME
  type over a window. A DropArea under it sees what it carries in the
  drag's `urls`, `text`, `formats` and `getDataAsString`, and may turn it
  down; the window system is told whether an area takes it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 400; height: 400
        property string log: ""
        DropArea {
          objectName: "files"; width: 200; height: 400
          onEntered: (drag) => {
            drag.accepted = drag.hasUrls && drag.urls.every(u => u.toString().startsWith("file:"))
            root.log += "files(" + drag.urls + " " + drag.accepted + ") "
          }
        }
        DropArea {
          objectName: "any"; x: 200; width: 200; height: 400
          onEntered: (drag) => root.log += "entered(" + drag.formats + ") "
          onExited: root.log += "exited "
          onDropped: (drop) => {
            root.log += "dropped(" + drop.urls + "|" + drop.text + "|"
              + drop.getDataAsString("application/x-example") + ") "
            drop.accept()
          }
        }
      }
      """

  @api-QtQuick.DragEvent.accepted @api-QtQuick.DragEvent.urls
  Scenario: An area that takes only files turns a link down
    When another application drags the URL "https://example.com/page" over 50, 50
    Then the drag from another application is not taken
    And the property "log" of "root" is "files(https://example.com/page false) "
    When the drag from another application is let go
    Then the drag from another application is not taken

  @api-QtQuick.DragEvent.urls
  Scenario: An area that takes only files takes a file
    When another application drags the URL "file:///home/a.txt" over 50, 50
    Then the drag from another application is taken
    And the property "log" of "root" is "files(file:///home/a.txt true) "

  @api-QtQuick.DropArea.dropped @api-QtQuick.DragEvent.text
  Scenario: Dropped text is the drop's text
    When another application drags the text "hello" over 250, 50
    Then the drag from another application is taken
    When the drag from another application is let go
    Then the drag from another application is taken
    And the property "log" of "root" is "entered(text/plain) dropped(|hello|) "

  @api-QtQuick.DragEvent.formats @api-QtQuick.DragEvent.getDataAsString
  Scenario: A drag carries every format the other application offers
    When another application drags this over 250, 50:
      | format                | data                     |
      | text/uri-list         | https://example.com/page |
      | text/plain            | a page                   |
      | application/x-example | 42                       |
    And the drag from another application is let go
    Then the property "log" of "root" is "entered(text/uri-list,text/plain,application/x-example) dropped(https://example.com/page|a page|42) "

  @api-QtQuick.DropArea.exited
  Scenario: A drag that moves away from the areas leaves them
    When another application drags the text "hello" over 250, 50
    And the drag from another application moves to 250, 500
    Then the drag from another application is not taken
    When the drag from another application leaves the window
    Then the property "log" of "root" is "entered(text/plain) exited "

  @api-QtQuick.DropArea.dropped
  Scenario: A drag is dropped on the area under the place it was let go
    When another application drags the URL "file:///home/a.txt" over 50, 50
    And the drag from another application moves to 250, 350
    And the drag from another application is let go
    Then the drag from another application is taken
    And the property "log" of "root" is "files(file:///home/a.txt true) entered(text/uri-list) dropped(file:///home/a.txt||) "
