# Sources:
#   docs/ARCHITECTURE.md (sections 12.1 and 6.3)
#   docs/research/03-runtime-concurrency-hot-reload.md
# Hot reload is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: A reload that fails
  A change that cannot be applied leaves the application as it was and says
  where the mistake is. Mending the file applies it and takes the message away.

  Rule: A file that does not compile leaves the application as it was

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          property int taps: 0
        }
        """
      When the property "taps" of "root" is set to 3

    Scenario: A syntax error is reported with its file and line
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20 +
          property int taps: 0
        }
        """
      And a reload happens
      Then the reload error is in "Scenario.qml" at line 5
      And the property "width" of "root" is 10
      And the property "taps" of "root" is 3

    Scenario: A type nobody knows is reported with its file and line
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20
          property int taps: 0
          Rectangel {}
        }
        """
      And a reload happens
      Then the reload error is in "Scenario.qml" at line 6
      And the reload error matches "Rectangel"
      And the property "width" of "root" is 10

    Scenario: No error is reported while nothing has failed
      Then there is no reload error

  Rule: A file that did not compile when the application started holds nothing back

    Background:
      Given the file "Broken.qml":
        """qml
        import QtQuick
        Item {
          NoSuchThing { }
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          property int taps: 0
        }
        """
      When the property "taps" of "root" is set to 3

    Scenario: Another file's change is applied
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20
          property int taps: 0
        }
        """
      And a reload happens
      Then there is no reload error
      And the property "width" of "root" is 20
      And the property "taps" of "root" is 3

    Scenario: The file mended can be used
      When the file "Broken.qml" is changed to:
        """qml
        import QtQuick
        Rectangle { objectName: "mended" }
        """
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          property int taps: 0
          Broken { }
        }
        """
      And a reload happens
      Then there is no reload error
      And the object "mended" exists
      And the property "taps" of "root" is 3

    Scenario: A save that leaves the file broken is reported
      When the file "Broken.qml" is changed to:
        """qml
        import QtQuick
        Item {
          NorThis { }
        }
        """
      And a reload happens
      Then the reload error is in "Broken.qml" at line 3
      And the property "taps" of "root" is 3

  Rule: A file that is removed while it is used leaves the application as it was

    Scenario: The missing file is reported
      Given the file "Badge.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "badge" }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Badge {}
        }
        """
      When the file "Badge.qml" is removed
      And a reload happens
      Then the reload error matches "Badge"
      And the object "badge" exists

  Rule: A change that compiles and cannot be applied leaves the application as it was

    Scenario: An object that cannot be created is reported
      Given the file "Badge.qml":
        """qml
        import QtQuick
        Rectangle { objectName: "badge"; width: 10 }
        """
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property int taps: 0
          Badge {}
        }
        """
      When the property "taps" of "root" is set to 3
      And the file "Badge.qml" is changed to:
        """qml
        import QtQuick
        Rectangle {
          objectName: "badge"; width: 20
          required property string label
        }
        """
      And a reload happens
      Then the reload error matches "label"
      And the property "width" of "badge" is 10
      And the property "taps" of "root" is 3

  Rule: Mending the file applies it and takes the message away

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          property int taps: 0
        }
        """
      When the property "taps" of "root" is set to 3
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20 +
          property int taps: 0
        }
        """
      And a reload happens

    Scenario: The mended file is applied
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20
          property int taps: 0
        }
        """
      And a reload happens
      Then there is no reload error
      And the property "width" of "root" is 20
      And the property "taps" of "root" is 3

    Scenario: Going back to the file as it was takes the message away
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
          property int taps: 0
        }
        """
      And a reload happens
      Then there is no reload error
      And the property "width" of "root" is 10

    Scenario: A second mistake replaces the first message
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Rectangel {}
        }
        """
      And a reload happens
      Then the reload error is in "Scenario.qml" at line 4

  Rule: The error of an entry that cannot be made stands until the entry can be

    The window has nothing to show while its entry does not compile, so a
    save that leaves the entry as it is, of a file the entry never uses,
    takes nothing of the error away. Nor does an entry after it that cannot
    be made either: the first entry's mistake is why, as at the start.

    Background:
      Given the file "app/Main.qml":
        """qml
        import QtQuick
        Item {
          objectName: "main"
          width: 20 +
        }
        """
      And the file "app/Notes.qml":
        """qml
        import QtQuick
        Item { }
        """
      And the file "app/Badge.qml":
        """qml
        import QtQuick
        Item { required property string label }
        """
      And the file "app/Later.qml":
        """qml
        import QtQuick
        Item {
          objectName: "later"
          Badge {}
        }
        """
      And the QML:
        """qml
        import QtQuick
        Item { objectName: "root" }
        """

    Scenario: A save of a file the entry does not use leaves the error
      When the application watches the roots "app" for the entries "Main"
      Then the reload error is in "Main.qml" at line 5
      When the file "app/Notes.qml" is saved on disk as:
        """qml
        import QtQuick
        Item { width: 3 }
        """
      Then the reload error is in "Main.qml" at line 5
      And the object "main" does not exist

    Scenario: An entry after it that cannot be made does not take the error's place
      When the application watches the roots "app" for the entries "Main, Later"
      Then the reload error is in "Main.qml" at line 5
      When the file "app/Notes.qml" is saved on disk as:
        """qml
        import QtQuick
        Item { width: 3 }
        """
      Then the reload error is in "Main.qml" at line 5
      And the object "later" does not exist

    Scenario: A mistake made and undone in another file gives the entry's error back
      When the application watches the roots "app" for the entries "Main"
      And the file "app/Notes.qml" is saved on disk as:
        """qml
        import QtQuick
        Item {
        """
      Then the reload error is in "Notes.qml" at line 3
      When the file "app/Notes.qml" is saved on disk as:
        """qml
        import QtQuick
        Item { }
        """
      Then the reload error is in "Main.qml" at line 5
      And the object "main" does not exist

    Scenario: The entry mended is shown and the error goes
      When the application watches the roots "app" for the entries "Main"
      And the file "app/Main.qml" is saved on disk as:
        """qml
        import QtQuick
        Item { objectName: "main" }
        """
      Then the object "main" exists
      And there is no reload error
