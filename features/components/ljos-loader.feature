# Sources:
#   docs/DIALECT.md §4.11 (a component type is a value), §4.14 (Loader)
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#source-prop
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#sourceComponent-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# `Ljos.Ui` is Ljós's own module, so its files are `@ljos-only`. Each one holds the numbers of the
# Qt file above it, which runs on both. A Loader sizes itself to its item when it is given no size,
# so its width and height are the page's. A Loader's change handler hears the component it is made
# with as well as the one it is given later: two changes for one change made (qml6).

Feature: A Loader shows a component, named as a type
  Qt's Loader shows a file named by a string, `source: "Page.qml"`,
  or a component, `sourceComponent`. In Ljós's API a component is a
  value and a type is one: `component: Page` shows the page, and no
  file is named by a string. A Qt file whose strings are written out
  is lowered to that; one that makes its string as it runs is kept.

  Background:
    Given the file "Page.qml":
      """qml
      import QtQuick
      Rectangle { width: 40; height: 30; color: "#336699" }
      """

  Rule: A Loader shows the component it is given

    @qt-parity
    Scenario: A Qt Loader shows the file its source names
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 100; height: 60
          property int loads: 0
          Loader { objectName: "loader"; source: "Page.qml"; onLoaded: root.loads += 1 }
        }
        """
      Then the property "status" of "loader" is 1
      And the property "loads" of "root" is 1
      And the property "width" of "loader" is 40
      And the property "height" of "loader" is 30
      And the pixel at 20, 15 of the window is "#336699"

    @ljos-only
    Scenario: A Loader of Ljós's API shows the type its component names
      Given the QML:
        """qml
        import Ljos.Ui
        Item {
          id: root; objectName: "root"; width: 100; height: 60
          property int loads: 0
          Loader { objectName: "loader"; component: Page; onLoaded: root.loads += 1 }
        }
        """
      Then the property "status" of "loader" is 1
      And the property "loads" of "root" is 1
      And the property "width" of "loader" is 40
      And the property "height" of "loader" is 30
      And the pixel at 20, 15 of the window is "#336699"

    @ljos-only
    Scenario: A component property takes a type as its value
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"; width: 100; height: 60
          property Component shown: Page
          Loader { objectName: "loader"; component: root.shown }
        }
        """
      Then the property "status" of "loader" is 1
      And the property "width" of "loader" is 40
      And the pixel at 20, 15 of the window is "#336699"

    # A type written where a component goes is the component of that type for TypeScript too
    # (features/tooling/check-typescript.feature): it is not a value of the script's.
    @ljos-only @headless
    Scenario: TypeScript reads a type given as a component as the checker does
      Given the file "Main.qml":
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root
          property Component shown: Page
          Loader { component: Page }
          Loader { component: root.shown }
        }
        """
      When the files are checked with TypeScript
      Then TypeScript reports nothing

    @qt-parity
    Scenario: A Qt Loader's sourceComponent is changed for another
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 100; height: 60
          property int changes: 0
          function grow() { loader.sourceComponent = big }
          Component { id: small; Rectangle { width: 20; height: 10; color: "#c03030" } }
          Component { id: big; Rectangle { width: 60; height: 40; color: "#30a030" } }
          Loader {
            id: loader; objectName: "loader"; sourceComponent: small
            onSourceComponentChanged: root.changes += 1
          }
        }
        """
      Then the property "width" of "loader" is 20
      When the method "grow" of "root" is called
      Then the property "width" of "loader" is 60
      And the property "height" of "loader" is 40
      And the property "changes" of "root" is 2
      And the pixel at 50, 30 of the window is "#30a030"

    @ljos-only
    Scenario: A Loader of Ljós's API has its component changed for another
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          id: root; objectName: "root"; width: 100; height: 60
          property int changes: 0
          function grow() { loader.component = big }
          Component { id: small; Rectangle { width: 20; height: 10; fill: "#c03030" } }
          Component { id: big; Rectangle { width: 60; height: 40; fill: "#30a030" } }
          Loader {
            id: loader; objectName: "loader"; component: small
            onComponentChanged: root.changes += 1
          }
        }
        """
      Then the property "width" of "loader" is 20
      When the method "grow" of "root" is called
      Then the property "width" of "loader" is 60
      And the property "height" of "loader" is 40
      And the property "changes" of "root" is 2
      And the pixel at 50, 30 of the window is "#30a030"

  Rule: A Qt Loader is lowered when what it loads is written out

    # `component` is the lowered file's name for what the Loader shows: a file kept as Qt's has no
    # such property, so the change is seen through it only when the file was lowered.
    @ljos-only
    Scenario: A Qt Loader lowered shows the same page and takes the same change
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"; width: 100; height: 60
          property int changes: 0
          function grow() { second.sourceComponent = big }
          Component { id: big; Rectangle { width: 60; height: 40; color: "#30a030" } }
          Loader { objectName: "loader"; source: "Page.qml" }
          Loader {
            id: second; objectName: "second"; y: 30
            onSourceComponentChanged: root.changes += 1
          }
        }
        """
      Then the property "component" of "second" is null
      And the property "width" of "loader" is 40
      And the pixel at 20, 15 of the window is "#336699"
      When the method "grow" of "root" is called
      Then the property "width" of "second" is 60
      And the property "component" of "second" is not null
      And the property "changes" of "root" is 1

    @ljos-only @headless
    Scenario: A Loader of a file beside it would be lowered with that file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          width: 100; height: 60
          Loader { source: "Page.qml" }
          Loader { x: 50; sourceComponent: dot }
          Component { id: dot; Rectangle { width: 10; height: 10; color: "#c03030" } }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        would lower ./Main.qml (5 edits)
        would lower ./Page.qml (2 edits)
        2 files, 2 lowered, 0 kept
        """
      When the author runs "ljos render . --out kept.png --settle 0 --compat keep"
      And the author runs "ljos render . --out lowered.png --settle 0 --compat lower"
      Then the pictures "kept.png" and "lowered.png" have the same pixels

    @ljos-only @headless
    Scenario: The lowered Loader names the page as a type
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          Loader { source: "Page.qml" }
          Loader { sourceComponent: dot; onSourceComponentChanged: {} }
          Component { id: dot; Item { } }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the file "Main.qml" is:
        """qml
        import Ljos.Ui
        import Ljos.Core
        Item {
          Loader { component: Page }
          Loader { component: dot; onComponentChanged: {} }
          Component { id: dot; Item { } }
        }
        """

  Rule: A Loader whose source is known only as it runs keeps its file

    @ljos-only @headless
    Scenario: A source made as the file runs keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          property string page: "Page"
          Loader { source: page + ".qml" }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:4:12: a `Loader`'s `source` is made as it runs, so what it loads is not known
        would lower ./Page.qml (2 edits)
        2 files, 1 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A source naming a file in another directory keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          Loader { source: "pages/Page.qml" }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:3:12: a `Loader`'s `source` is "pages/Page.qml", which is not a file beside this one
        would lower ./Page.qml (2 edits)
        2 files, 1 lowered, 1 kept
        """

    @ljos-only @headless
    Scenario: A source read as well as written keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
          readonly property url shown: loader.source
          Loader { id: loader; source: "Page.qml" }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:4:24: a `Loader`'s `source` is read or set as well as written
        kept ./Page.qml:1:1: it is used by `Main.qml`, which is kept: a `Loader`'s `source` is read or set as well as written
        2 files, 0 lowered, 2 kept
        """
