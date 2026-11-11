# Sources:
#   docs/ARCHITECTURE.md (section 12.1, goal 1)
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#source-prop
# Hot reload is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: A Loader shows what the file it loads now says
  Users change their own applications while they run. A Loader that loads
  a file by its URL shows the edited file after a reload, whether it loads
  at once or in the background, and however the URL is written. A Loader
  made again goes on loading what the application told it to.

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

  Rule: A Loader made again loads its item as setSource asked

    A file given a new signal is made again, not edited in place: its Loader
    is a new one, and loads what the old one was told to with the properties
    it was given, whether it loads at once or in the background.

    Scenario Outline: A loaded item keeps what it was given and what a script assigned, <how>
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { id: loader; objectName: "loader" }
          function show(later) {
            loader.asynchronous = later
            loader.setSource("Side.qml", { text: "given" })
          }
          function widen() { loader.item.width = 7 }
        }
        """
      When the method "show" of "root" is called with <later>
      And 100 milliseconds pass
      And the method "widen" of "root" is called
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          signal remade
          Loader { id: loader; objectName: "loader" }
          function show(later) {
            loader.asynchronous = later
            loader.setSource("Side.qml", { text: "given" })
          }
          function widen() { loader.item.width = 7 }
        }
        """
      And a reload happens
      And 100 milliseconds pass
      Then there is no reload error
      And the property "text" of "loader/side" is "given"
      And the property "width" of "loader/side" is 7

      Examples:
        | how            | later |
        | at once        | false |
        | asynchronously | true  |

    Scenario: A Loader that names the file itself is given the properties too
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { id: loader; objectName: "loader"; source: "Side.qml" }
          function show() { loader.setSource("Side.qml", { text: "given" }) }
        }
        """
      When the method "show" of "root" is called
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          signal remade
          Loader { id: loader; objectName: "loader"; source: "Side.qml" }
          function show() { loader.setSource("Side.qml", { text: "given" }) }
        }
        """
      And a reload happens
      Then there is no reload error
      And the property "text" of "loader/side" is "given"

    Scenario: An asynchronous Loader that names the file itself is given the properties too
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          Loader { id: loader; objectName: "loader"; asynchronous: true; source: "Side.qml" }
          function show() { loader.setSource("Side.qml", { text: "given" }) }
        }
        """
      When the method "show" of "root" is called
      And 100 milliseconds pass
      And the file "Scenario.qml" is changed to:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          signal remade
          Loader { id: loader; objectName: "loader"; asynchronous: true; source: "Side.qml" }
          function show() { loader.setSource("Side.qml", { text: "given" }) }
        }
        """
      And a reload happens
      And 100 milliseconds pass
      Then there is no reload error
      And the property "text" of "loader/side" is "given"
