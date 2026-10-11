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
