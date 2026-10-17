# Sources:
#   docs/ARCHITECTURE.md (section 12.1)
#   docs/research/03-runtime-concurrency-hot-reload.md
# Hot reload is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: When a changed file is reloaded
  The files of a running application are watched while it is developed. A
  change is applied once the files have been quiet for 120 milliseconds, so
  an editor that saves in several steps causes one reload. A save that
  changes nothing causes none.

  Rule: A change is applied once the files have been quiet for 120 milliseconds

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
        }
        """

    Scenario: A change is not applied at once
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20
        }
        """
      And 100 milliseconds pass
      Then the property "width" of "root" is 10
      And 0 reloads have happened

    Scenario: A change is applied after the quiet time
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20
        }
        """
      And 120 milliseconds pass
      Then the property "width" of "root" is 20
      And 1 reload has happened

    Scenario: Every save starts the quiet time again
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20
        }
        """
      And 100 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 30
        }
        """
      And 100 milliseconds pass
      Then the property "width" of "root" is 10
      When 20 milliseconds pass
      Then the property "width" of "root" is 30
      And 1 reload has happened

  Rule: A save that changes nothing causes no reload

    # "Nothing" is to the letter: the text saved below is the text of the
    # background, indentation included.
    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
        }
        """

    Scenario: The same text is saved again
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
        }
        """
      And 1 second passes
      Then 0 reloads have happened

    Scenario: A change is undone within the quiet time
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20
        }
        """
      And 50 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
        }
        """
      And 1 second passes
      Then 0 reloads have happened
      And the property "width" of "root" is 10

  Rule: The files are still watched after a reload that failed

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
        }
        """

    Scenario: A mended file is applied after its quiet time
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20 +
        }
        """
      And 120 milliseconds pass
      Then the reload error is in "Scenario.qml" at line 5
      And 0 reloads have happened
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20
        }
        """
      And 120 milliseconds pass
      Then there is no reload error
      And the property "width" of "root" is 20
      And 1 reload has happened

  Rule: A file emptied by a save that has not finished waits for the save

    # An editor that saves in place empties the file, then writes it. On a
    # busy disk the writing can come seconds after the quiet time; the file
    # is taken as it is once its writer has finished, and not on a timer.
    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 10
        }
        """

    Scenario: A file written long after it was emptied is applied without an error
      When the file "Scenario.qml" is emptied by a save that has not finished
      And 1000 milliseconds pass
      Then there is no reload error
      And 0 reloads have happened
      When the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 20
        }
        """
      And 120 milliseconds pass
      Then there is no reload error
      And the property "width" of "root" is 20
      And 1 reload has happened

    Scenario: A file saved empty is reported
      When the file "Scenario.qml" is changed to:
        """qml
        """
      And 120 milliseconds pass
      Then the reload error is in "Scenario.qml" at line 1
      And the property "width" of "root" is 10

    Scenario: A reload asked for takes a file being written as it is
      When the file "Scenario.qml" is emptied by a save that has not finished
      And a reload happens
      Then the reload error is in "Scenario.qml" at line 1
      And the property "width" of "root" is 10
