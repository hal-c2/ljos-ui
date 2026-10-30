# Sources:
#   docs/ARCHITECTURE.md sections 5.5 (the type dump) and 12.1 (development)
#   https://doc.qt.io/qt-6/qtqml-syntax-signals.html
# `ljos run` and `ljostypes.json` are Ljós's own: Qt has nothing to compare them with.

@ljos-only @headless
Feature: Running a project whose types are described by its ljostypes.json
  An application that registers types from Rust describes them in
  `ljostypes.json`. `ljos run` has none of that Rust, but it can show the
  interface: it reads the file named by `--types`, else `ljostypes.json` in
  the project's directory (the entry's directory, or `--project`), and looks
  no further. A file that is absent is nothing to say, and one that was
  found and cannot be read stops the run. The file's types are checked as
  the build checks them, and each one no Rust code provides stands in with
  the members it declares: its properties hold what they are given, its
  signals can be emitted and handled, and its methods do nothing and return
  `undefined`. The run says once which types stand in.

  Background:
    Given the file "ljostypes.json":
      """json
      {"format": 2, "modules": [{"uri": "App.Media", "versions": ["1.0"], "types": [
        {"name": "Player", "since": "1.0", "base": "QtObject", "baseModule": "QtQml",
         "enums": [{"name": "State", "keys": [["Stopped", 0], ["Playing", 7]]}],
         "creatable": true, "defaultProperty": null, "attached": null,
         "properties": [{"name": "volume", "type": "int", "kind": "value", "readonly": false,
           "final": true, "required": false, "constant": false, "group": false,
           "notify": "volumeChanged"}],
         "signals": [{"name": "finished", "parameters": [{"name": "position", "type": "real"}]}],
         "methods": [{"name": "seek", "parameters": [{"name": "to", "type": "real"}],
           "returnType": "bool"}]},
        {"name": "Wave", "since": "1.0", "base": "Item", "baseModule": "QtQuick",
         "creatable": true, "defaultProperty": null, "attached": null,
         "properties": [], "signals": [], "methods": []},
        {"name": "Clock", "since": "1.0", "base": "QtObject", "baseModule": "QtQml",
         "singleton": true, "creatable": false, "defaultProperty": null, "attached": null,
         "properties": [{"name": "zone", "type": "string", "kind": "value", "readonly": false,
           "final": true, "required": false, "constant": false, "group": false,
           "notify": "zoneChanged"}],
         "signals": [], "methods": []}]}]}
      """
    And the file "Main.qml":
      """qml
      import QtQuick
      import App.Media

      Item {
          objectName: "root"
          width: 200; height: 100
          property int heard: 0
          property real position: 0
          Player {
              id: player
              objectName: "player"
              volume: 3
              onVolumeChanged: root.heard += 1
              onFinished: root.position = position
          }
          Wave { objectName: "wave"; anchors.fill: parent }
          id: root
      }
      """

  Rule: The types the file describes stand in with the members it declares

    Scenario: The run says which types stand in
      When the author starts "ljos run Main.qml --headless --mcp=inspect"
      Then the application's errors mention "ljos: ljostypes.json: App.Media.Player, App.Media.Wave, App.Media.Clock stand in: no Rust code provides them"

    Scenario: A property holds what it is given and says it changed
      When the author starts "ljos run Main.qml --headless --mcp=inspect,eval"
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#player", "source": "volume = 9, [volume, root.heard]"}
        """
      Then the answer's "/value" is [9, 1]

    Scenario: A signal's handler is given its arguments by name
      When the author starts "ljos run Main.qml --headless --mcp=inspect,eval"
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#player", "source": "finished(2.5), root.position"}
        """
      Then the answer's "/value" is 2.5

    Scenario: A method does nothing and returns undefined
      When the author starts "ljos run Main.qml --headless --mcp=inspect,eval"
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#player", "source": "seek(4) === undefined"}
        """
      Then the answer's "/value" is true

    Scenario: A key of its enum has the number the file gives it
      When the author starts "ljos run Main.qml --headless --mcp=inspect,eval"
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#player", "source": "Player.Playing"}
        """
      Then the answer's "/value" is 7

    Scenario: One that inherits Item is laid out as an Item
      When the author starts "ljos run Main.qml --headless --mcp=inspect,eval"
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#wave", "source": "[width, height]"}
        """
      Then the answer's "/value" is [200, 100]

    Scenario: A singleton is one object that holds what it is given
      When the author starts "ljos run Main.qml --headless --mcp=inspect,eval"
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#player", "source": "Clock.zone = 'UTC', Clock.zone"}
        """
      Then the answer's "/value" is "UTC"

    Scenario: A save compiled against the file keeps knowing its types
      When the author starts "ljos run Main.qml --headless --watch --mcp=inspect,eval"
      And the file "Main.qml" is saved as:
        """qml
        import QtQuick
        import App.Media

        Player { objectName: "saved"; volume: 12 }
        """
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#saved", "source": "volume"}
        """
      Then the answer's "/value" is 12

  Rule: What the file does not describe is a mistake, as in a build

    Scenario: A handler of a signal the type does not have stops the run
      Given the file "Main.qml":
        """qml
        import App.Media

        Player { onStopped: 0 }
        """
      When the author runs "ljos run Main.qml --headless"
      Then the command exits with status 1
      And the command's output mentions "onStopped"

    Scenario: A change signal is one the file implies through its property
      Given the file "Main.qml":
        """qml
        import QtQml
        import App.Media

        Player { onVolumeChanged: 0; Component.onCompleted: Qt.quit() }
        """
      When the author runs "ljos run Main.qml --headless"
      Then the command exits with status 0

  Rule: Where the file is found

    Scenario: A file named on the command line is read instead of the project's
      Given the file "types/own.json":
        """json
        {"format": 2, "modules": [{"uri": "App.Media", "versions": ["1.0"], "types": [
          {"name": "Other", "since": "1.0", "base": "Item", "baseModule": "QtQuick",
           "creatable": true, "defaultProperty": null, "attached": null,
           "properties": [], "signals": [], "methods": []}]}]}
        """
      And the file "Main.qml":
        """qml
        import QtQml
        import App.Media

        Other { Component.onCompleted: Qt.quit() }
        """
      When the author runs "ljos run Main.qml --headless --types types/own.json"
      Then the command exits with status 0
      And the command's output mentions "App.Media.Other stand in"
      And the command's output does not mention "App.Media.Player"

    Scenario: The project's directory is the one that is looked in
      Given the file "ui/Main.qml":
        """qml
        import QtQml
        import App.Media

        Wave { Component.onCompleted: Qt.quit() }
        """
      When the author runs "ljos run ui/Main.qml --project . --headless"
      Then the command exits with status 0
      And the command's output mentions "stand in"

    Scenario: Without a project the entry's directory is the only one looked in
      Given the file "ui/Main.qml":
        """qml
        import App.Media

        Wave { }
        """
      When the author runs "ljos run ui/Main.qml --headless"
      Then the command exits with status 1
      And the command's output mentions "App.Media"
      And the command's output does not mention "stand in"

  Rule: A file that was found and cannot be used stops the run

    Scenario Outline: The run names the file and what is wrong with it
      Given the file "ljostypes.json":
        """json
        <text>
        """
      When the author runs "ljos run Main.qml --headless"
      Then the command exits with status 1
      And the command's output mentions "ljostypes.json"
      And the command's output mentions "<says>"

      Examples:
        | text                         | says                               |
        | not json                     | expected                           |
        | {"format": 1, "modules": []} | type dump format 1 where 2 is read |

    Scenario: A file named on the command line must be there
      When the author runs "ljos run Main.qml --headless --types absent.json"
      Then the command exits with status 1
      And the command's output mentions "absent.json"

    Scenario: A base nothing describes is named
      Given the file "ljostypes.json":
        """json
        {"format": 2, "modules": [{"uri": "App.Media", "versions": ["1.0"], "types": [
          {"name": "Player", "since": "1.0", "base": "Nowhere", "creatable": true,
           "defaultProperty": null, "attached": null,
           "properties": [], "signals": [], "methods": []}]}]}
        """
      When the author runs "ljos run Main.qml --headless"
      Then the command exits with status 1
      And the command's output mentions "its base Nowhere"
