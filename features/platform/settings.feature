# Sources:
#   https://doc.qt.io/qt-6/qml-qtcore-settings.html
#   https://doc.qt.io/qt-6/qsettings.html#platform-specific-notes
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Settings that outlive the application
  A Settings object keeps its own properties, and values set by key, in a
  file. A property starts from the file and is written back after it
  changes, without the application waiting for the disk.

  Rule: Reading a key

    @qt-parity @api-QtCore.Settings.value
    Scenario: A key that was never stored has no value
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item {
          objectName: "o"
          Settings { id: s; category: "ljos-never-stored" }
          property bool missing: s.value("nothing") === undefined
          property var fallback: s.value("nothing", 42)
        }
        """
      Then the property "missing" of "o" is true
      And the property "fallback" of "o" is 42

    @qt-parity @api-QtCore.Settings.location
    Scenario: A Settings starts with no location of its own
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item { Settings { objectName: "s"; category: "ljos-never-stored" } }
        """
      Then the property "location" of "s" is ""
      And the property "category" of "s" is "ljos-never-stored"

  Rule: A file of the application's own

    @qt-parity @api-QtCore.Settings
    Scenario: A property starts from the file
      Given the text file "app.conf":
        """
        [General]
        width=7
        name=Ada
        """
      And the QML:
        """qml
        import QtQuick
        import QtCore
        Item {
          Settings {
            objectName: "s"
            location: Qt.resolvedUrl("app.conf")
            property int width: 5
            property string name: "nobody"
            property real scale: 1.5
          }
        }
        """
      Then the property "width" of "s" is 7
      And the property "name" of "s" is "Ada"
      And the property "scale" of "s" is 1.5

    @qt-parity
    Scenario: A category reads the section of its name
      Given the text file "app.conf":
        """
        [General]
        width=1
        [window]
        width=640
        """
      And the QML:
        """qml
        import QtQuick
        import QtCore
        Item {
          Settings {
            objectName: "s"
            location: Qt.resolvedUrl("app.conf")
            category: "window"
            property int width: 5
          }
        }
        """
      Then the property "width" of "s" is 640

    @qt-parity @api-QtCore.Settings.setValue
    Scenario Outline: A value set by key reads back as <value>
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item {
          objectName: "o"
          property var got
          Settings { id: s; location: Qt.resolvedUrl("app.conf") }
          Component.onCompleted: { s.setValue("k", <value>); got = s.value("k") }
        }
        """
      Then the property "got" of "o" is <value>

      Examples:
        | value  |
        | 3      |
        | "text" |
        | true   |
        | [1, 2] |

    @qt-parity @api-QtCore.Settings.sync
    Scenario: Another Settings of the file sees a change once it is synced
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item {
          objectName: "o"
          property var before
          property var after
          Settings { id: a; location: Qt.resolvedUrl("app.conf"); property int width: 5 }
          Settings { id: b; location: Qt.resolvedUrl("app.conf") }
          Component.onCompleted: { a.width = 9; a.sync(); after = b.value("width") }
        }
        """
      # Read back from the file, the number is text.
      Then the property "after" of "o" is "9"

  # The oracle runs in a directory Ljós's tests cannot read back from.
  @ljos-only
  Rule: Writing the file

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item {
          Settings {
            objectName: "s"
            location: Qt.resolvedUrl("app.conf")
            property int width: 5
            property string name: "nobody"
          }
        }
        """

    Scenario: A changed property is written half a second later
      When the property "width" of "s" is set to 9
      And 600 milliseconds pass
      Then the text file "app.conf" has the line "width=9"
      And the text file "app.conf" has the line "[General]"

    Scenario: Nothing is written while the changes keep coming
      When the property "width" of "s" is set to 9
      And 100 milliseconds pass
      Then the text file "app.conf" does not exist

    Scenario: sync() writes at once
      When the property "name" of "s" is set to "Ada"
      And the method "sync" of "s" is called
      Then the text file "app.conf" has the line "name=Ada"

    Scenario: A Settings of the same file made later starts from what was written
      Given the file "Later.qml":
        """qml
        import QtQuick
        import QtCore
        Settings {
          objectName: "later"
          location: Qt.resolvedUrl("app.conf")
          property int width: 5
        }
        """
      When the property "width" of "s" is set to 12
      And 600 milliseconds pass
      And "Later.qml" is loaded
      Then the property "width" of "later" is 12

    Scenario: Settings that change nothing write nothing
      When 1 second passes
      Then the text file "app.conf" does not exist
