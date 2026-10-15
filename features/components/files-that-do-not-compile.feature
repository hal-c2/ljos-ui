# Sources:
#   https://doc.qt.io/qt-6/qtqml-documents-definetypes.html
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html
#   https://doc.qt.io/qt-6/qml-qtqml-component.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A file of the application that does not compile
  An application's files are read when something makes an object of them. A
  file with a mistake in it stops nothing until then: the application runs,
  and whatever makes an object of that file is told what the mistake is.

  Background:
    Given the file "Broken.qml":
      """qml
      import QtQuick
      Item {
        NoSuchThing { }
      }
      """
    And the file "Unfinished.qml":
      """qml
      import QtQuick
      Item {
        width: 20 +
      }
      """

  Rule: A file nobody makes an object of is not read

    Scenario: The application runs beside files that do not compile
      Given the QML:
        """qml
        import QtQuick
        Item { objectName: "root"; property int n: 1 }
        """
      Then the property "n" of "root" is 1
      And the log has no warnings

  Rule: Making an object of the file says what is wrong with it

    Scenario: A Loader of the file is in error and says where the mistake is
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { objectName: "loader"; source: "Broken.qml" }
        }
        """
      Then the property "status" of "loader" is 3
      And the log has a warning matching "Broken\.qml:3:\d+: .*NoSuchThing.? is not a type"

    Scenario: The component of the file is in error and says why
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property var made: Qt.createComponent("Unfinished.qml")
          property bool failed: made.status === Component.Error
          property bool said: made.errorString().indexOf("Unfinished.qml:4:") >= 0
        }
        """
      Then the property "failed" of "root" is true
      And the property "said" of "root" is true

    Scenario: A file that names the type in its objects does not load
      Given the file "Uses.qml":
        """qml
        import QtQuick
        Item { Broken { } }
        """
      Then loading "Uses.qml" is rejected
