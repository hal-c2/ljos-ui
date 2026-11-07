# Sources:
#   docs/DIALECT.md sections 4.1 (the modules), 4.4 (native types) and 6.3 (what keeps a file)
#   docs/ARCHITECTURE.md section 5.5 (the type dump)
# A program's own types, `ljostypes.json` and lowering a file to Ljós's API are Ljós's own: Qt has
# nothing to compare them with. The module, its types and its singleton are cases of their own.

@ljos-only @headless
Feature: Lowering a file that imports the program's own types
  A program that registers types from Rust describes them in
  `ljostypes.json`, and the compat pass reads that description as it
  reads Qt's: `ljos migrate` takes the file named by `--types`, else the
  one in the first directory it is given (of a file, its own), and a
  lowered run takes the one the checker does. One it cannot read stops it
  before it writes anything. A file that imports a module the description
  has is lowered with its import as written, beside `import Ljos.Ui`. Its
  types are typed as Qt's are: a member a native type declares keeps its
  name, its singletons' members are typed through what they hold, and a
  member of the toolkit type it is built on is that type's. A name Ljós
  renames there keeps the file, as the program's type has it under Qt's
  name. A type the description lacks keeps the file too. Of the imports
  that have a type's name, the last is the one it is.

  Background:
    Given the file "ljostypes.json":
      """json
      {"format": 2, "modules": [{"uri": "Studio.Kit", "versions": ["1.0"], "types": [
        {"name": "Tone", "since": "1.0", "base": "QtObject", "baseModule": "QtQml",
         "creatable": true, "defaultProperty": null, "attached": null, "signals": [], "methods": [],
         "properties": [
           {"name": "color", "type": "color", "kind": "value", "readonly": false, "final": true,
            "required": false, "constant": false, "group": false, "notify": "colorChanged"}]},
        {"name": "Palette", "since": "1.0", "base": "QtObject", "baseModule": "QtQml",
         "creatable": true, "defaultProperty": null, "attached": null, "signals": [], "methods": [],
         "properties": [
           {"name": "accent", "type": "Tone", "kind": "object", "readonly": true, "final": true,
            "required": false, "constant": true, "group": false, "notify": null}]},
        {"name": "Swatch", "since": "1.0", "base": "Rectangle", "baseModule": "QtQuick",
         "creatable": true, "defaultProperty": null, "attached": null, "signals": [], "methods": [],
         "properties": [
           {"name": "label", "type": "string", "kind": "value", "readonly": false, "final": true,
            "required": false, "constant": false, "group": false, "notify": "labelChanged"}]},
        {"name": "Look", "since": "1.0", "base": "QtObject", "baseModule": "QtQml", "singleton": true,
         "creatable": false, "defaultProperty": null, "attached": null, "signals": [], "methods": [],
         "properties": [
           {"name": "palette", "type": "Palette", "kind": "object", "readonly": true, "final": true,
            "required": false, "constant": true, "group": false, "notify": null},
           {"name": "card", "type": "Swatch", "kind": "object", "readonly": true, "final": true,
            "required": false, "constant": true, "group": false, "notify": null},
           {"name": "spacing", "type": "int", "kind": "value", "readonly": false, "final": true,
            "required": false, "constant": false, "group": false, "notify": "spacingChanged"}]}]}]}
      """

  Rule: A module the description has is lowered with its import as written

    Scenario: A file that imports the program's own module is lowered
      Given the file "Main.qml":
        """qml
        import QtQuick
        import Studio.Kit 1.0

        Rectangle {
            color: "#204080"
            width: Look.spacing * 10
            Swatch { label: "Sea"; radius: 4 }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (2 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui
        import Studio.Kit 1.0

        Rectangle {
            fill: "#204080"
            width: Look.spacing * 10
            Swatch { label: "Sea"; radius: 4 }
        }
        """

    Scenario: A singleton's members are typed by what they hold
      Given the file "Main.qml":
        """qml
        import QtQuick
        import Studio.Kit

        Rectangle {
            color: Look.palette.accent.color
            Text { color: Look.palette.accent.color }
        }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (2 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui
        import Studio.Kit

        Rectangle {
            fill: Look.palette.accent.color
            Text { color: Look.palette.accent.color }
        }
        """

    Scenario: A directory with no description keeps the file, and one named on the command line lowers it
      Given the file "plain/Main.qml":
        """qml
        import QtQuick
        import Studio.Kit
        Item { width: Look.spacing }
        """
      When the author runs "ljos migrate plain"
      Then the command prints:
        """
        kept plain/Main.qml:2:1: it imports `Studio.Kit`, which is not lowered yet
        1 files, 0 lowered, 1 kept
        """
      When the author runs "ljos migrate --types ljostypes.json plain"
      Then the command prints:
        """
        would lower plain/Main.qml (1 edits)
        1 files, 1 lowered, 0 kept
        """

    Scenario: Only the first path given is looked in for a description
      Given the file "plain/Main.qml":
        """qml
        import QtQuick
        import Studio.Kit
        Item { width: Look.spacing }
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        import Studio.Kit
        Item { height: Look.spacing }
        """
      When the author runs "ljos migrate plain Main.qml"
      Then the command prints:
        """
        kept Main.qml:2:1: it imports `Studio.Kit`, which is not lowered yet
        kept plain/Main.qml:2:1: it imports `Studio.Kit`, which is not lowered yet
        2 files, 0 lowered, 2 kept
        """
      When the author runs "ljos migrate Main.qml plain"
      Then the command prints:
        """
        would lower Main.qml (1 edits)
        would lower plain/Main.qml (1 edits)
        2 files, 2 lowered, 0 kept
        """

  Rule: A description that cannot be read stops the command before any file is written

    Scenario: A description named by `--types` that is not there exits 2 and writes nothing
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle { color: "#204080" }
        """
      When the author runs "ljos migrate --write --types missing.json ."
      Then the command prints:
        """
        missing.json: No such file or directory (os error 2)
        """
      And the command exits with status 2
      And the file "Main.qml" is:
        """qml
        import QtQuick
        Rectangle { color: "#204080" }
        """

    Scenario: A description found beside the files that is not JSON exits 2, naming its directory, and writes nothing
      Given the file "broken/ljostypes.json":
        """json
        {"format": 2, "modules": [
        """
      And the file "broken/Main.qml":
        """qml
        import QtQuick
        Rectangle { color: "#204080" }
        """
      When the author runs "ljos migrate --write broken"
      Then the command prints:
        """
        broken/ljostypes.json: EOF while parsing a list at line 2 column 0
        """
      And the command exits with status 2
      And the file "broken/Main.qml" is:
        """qml
        import QtQuick
        Rectangle { color: "#204080" }
        """

  Rule: A native type's members are what the description and its toolkit base say

    Scenario: A member Ljós renames on the toolkit type a native type is built on keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        import Studio.Kit

        Item {
            Swatch { color: "#336699" }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:5:14: `color` of `Swatch` is Rectangle's, which Ljos.Ui calls `fill`, and the program's type has it as `color`
        1 files, 0 lowered, 1 kept
        """

    Scenario: The same name read off a singleton's native value keeps the file too
      Given the file "Main.qml":
        """qml
        import QtQuick
        import Studio.Kit

        Text { color: Look.card.color }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:4:25: `color` of `Swatch` is Rectangle's, which Ljos.Ui calls `fill`, and the program's type has it as `color`
        1 files, 0 lowered, 1 kept
        """

    Scenario: A type the description lacks keeps the file
      Given the file "Main.qml":
        """qml
        import QtQuick
        import Studio.Kit

        Item {
            Gauge { }
        }
        """
      When the author runs "ljos migrate ."
      Then the command prints:
        """
        kept ./Main.qml:5:5: `Gauge` is neither a file beside it nor a type of `Studio.Kit` as the program describes it
        1 files, 0 lowered, 1 kept
        """

  Rule: Of the imports that have a type's name, the last is the one it is

    Scenario: A type of the program's own module named as a Qt type is the program's when it is imported after QtQuick
      Given the file "own/ljostypes.json":
        """json
        {"format": 2, "modules": [{"uri": "Studio.Print", "versions": ["1.0"], "types": [
          {"name": "Text", "since": "1.0", "base": "QtObject", "baseModule": "QtQml",
           "creatable": true, "defaultProperty": null, "attached": null, "signals": [], "methods": [],
           "properties": [
             {"name": "text", "type": "string", "kind": "value", "readonly": false, "final": true,
              "required": false, "constant": false, "group": false, "notify": "textChanged"}]}]}]}
        """
      And the file "own/After.qml":
        """qml
        import QtQuick
        import Studio.Print

        Rectangle {
            color: "#204080"
            Text { text: color }
        }
        """
      And the file "own/Before.qml":
        """qml
        import Studio.Print
        import QtQuick

        Rectangle {
            color: "#204080"
            Text { text: color }
        }
        """
      When the author runs "ljos migrate --write own"
      Then the command prints:
        """
        lowered own/After.qml (3 edits)
        lowered own/Before.qml (2 edits)
        2 files, 2 lowered, 0 kept
        """
      And the file "own/After.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui
        import Studio.Print

        Rectangle {
            fill: "#204080"
            Text { text: fill }
        }
        """
      And the file "own/Before.qml" is:
        """qml
        import Studio.Print
        pragma Scripts: JavaScript; import Ljos.Ui

        Rectangle {
            fill: "#204080"
            Text { text: color }
        }
        """

  Rule: A lowered run reads the description the checker reads

    Scenario: `ljos run --compat lower` lowers a file that imports the program's own module
      Given the file "Main.qml":
        """qml
        import QtQuick
        import Studio.Kit

        Item {
            width: 100; height: 60
            Rectangle { objectName: "card"; anchors.fill: parent; color: "#336699" }
            Swatch { objectName: "swatch"; label: "Sea" }
        }
        """
      When the author starts "ljos run Main.qml --headless --compat lower --mcp=inspect,eval"
      And the agent calls the tool "app_eval" with:
        """json
        {"target": "#card", "source": "String(fill)"}
        """
      Then the answer's "/value" is "#336699"
      When the agent calls the tool "app_eval" with:
        """json
        {"target": "#swatch", "source": "label"}
        """
      Then the answer's "/value" is "Sea"
