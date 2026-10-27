# Sources:
#   docs/ARCHITECTURE.md (section 12.1, goal 9)
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#source-prop
# Hot reload is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: A Loader shows what the file it loads now says
  Users change their own applications while they run. A Loader that loads
  a file by its URL shows the edited file after a reload, whether it loads
  at once or in the background, and however the URL is written.

  Background:
    Given the file "Side.qml":
      """qml
      import QtQuick
      Text { objectName: "side"; text: "old" }
      """

  Rule: An edited file a Loader loads is loaded again

    Scenario: A Loader given the file by its name shows the edited file
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { objectName: "loader"; source: "Side.qml" }
        }
        """
      When the file "Side.qml" is changed to:
        """qml
        import QtQuick
        Text { objectName: "side"; text: "new" }
        """
      And a reload happens
      Then there is no reload error
      And the property "text" of "side" is "new"

    Scenario: A Loader given a resolved URL shows the edited file
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { objectName: "loader"; source: Qt.resolvedUrl("Side.qml") }
        }
        """
      When the file "Side.qml" is changed to:
        """qml
        import QtQuick
        Text { objectName: "side"; text: "new" }
        """
      And a reload happens
      Then there is no reload error
      And the property "text" of "side" is "new"

    Scenario: An asynchronous Loader shows the edited file
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { objectName: "loader"; asynchronous: true; source: Qt.resolvedUrl("Side.qml") }
        }
        """
      When 100 milliseconds pass
      And the file "Side.qml" is changed to:
        """qml
        import QtQuick
        Text { objectName: "side"; text: "new" }
        """
      And a reload happens
      And 100 milliseconds pass
      Then there is no reload error
      And the property "text" of "side" is "new"

    Scenario: A script of the edited file runs its new body
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { objectName: "loader"; asynchronous: true; source: Qt.resolvedUrl("Side.qml") }
        }
        """
      When 100 milliseconds pass
      And the file "Side.qml" is changed to:
        """qml
        import QtQuick
        Text {
          objectName: "side"
          function word() { return "new" }
          text: word()
        }
        """
      And a reload happens
      And 100 milliseconds pass
      Then there is no reload error
      And the property "text" of "side" is "new"
