# Sources:
#   docs/adr/0003-blocks-and-shells.md parts 1 and 2
#   docs/ARCHITECTURE.md sections 5.5 and 19
#   https://doc.qt.io/qt-6/qtqml-modules-qmldir.html
# The block registry is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Listing the functional blocks of a project
  A functional block is a type that does the work and publishes what it
  knows: a player that plays, a list that loads. A module marks its QML
  blocks with a `# block` comment in its qmldir file, which Qt reads past;
  a type written in Rust is marked where it is declared and arrives in the
  project's type dump. `blocks_list` lists them without running anything,
  each with what it publishes (state), what a skin may set (inputs), what
  it does (actions), what it reports (events) and where others may add
  pieces (slots).

  Background:
    Given the authoring tools are connected
    And the file "App/Media/qmldir":
      """
      module App.Media
      Player 1.0 Player.qml
      # block Player 1.0 Player.qml Media "Plays one source"
      Waveform 1.0 Waveform.qml
      """
    And the file "App/Media/Player.qml":
      """qml
      import QtQuick
      Item {
          readonly property bool playing: false
          property url source
          property list<Contribution> toolbar
          signal finished(real position)
          function play() { }
          function seek(to: real): bool { return true }
      }
      """
    And the file "App/Media/Waveform.qml":
      """qml
      import QtQuick
      Item { }
      """

  Scenario: A QML block's contract is what its file declares
    When the agent calls the tool "blocks_list" with:
      """json
      {}
      """
    Then the answer's "/blocks/0/name" is "Player"
    And the answer's "/blocks/0/module" is "App.Media"
    And the answer's "/blocks/0/origin" is {"kind": "qml", "file": "App/Media/Player.qml"}
    And the answer's "/blocks/0/category" is "Media"
    And the answer's "/blocks/0/summary" is "Plays one source"
    And the answer's "/blocks/0/state" is [{"name": "playing", "type": "bool", "notify": "playingChanged"}]
    And the answer's "/blocks/0/inputs/0/name" is "source"
    And the answer's "/blocks/0/actions/1" is {"name": "seek", "params": [{"name": "to", "type": "real"}], "returns": "bool"}
    And the answer's "/blocks/0/events/0/name" is "finished"
    And the answer's "/blocks/0/slots" is [{"name": "toolbar", "accepts": "Contribution"}]

  Scenario: A type its module does not mark as a block is not listed
    When the agent calls the tool "blocks_list" with:
      """json
      {}
      """
    Then the answer's "/blocks" has 1 entry
    And the answer's "/errors" has 0 entries

  Scenario: Only the blocks of the module asked for are listed
    Given the file "App/Chat/qmldir":
      """
      module App.Chat
      # block Thread 1.0 Thread.qml
      """
    And the file "App/Chat/Thread.qml":
      """qml
      import QtQuick
      Item { readonly property int unread: 0 }
      """
    When the agent calls the tool "blocks_list" with:
      """json
      {"module": "App.Chat"}
      """
    Then the answer's "/blocks" has 1 entry
    And the answer's "/blocks/0/name" is "Thread"

  Scenario: A block whose file is missing is reported and the others are listed
    Given the file "App/Chat/qmldir":
      """
      module App.Chat
      # block Thread 1.0 Thread.qml
      """
    When the agent calls the tool "blocks_list" with:
      """json
      {}
      """
    Then the answer's "/blocks" has 1 entry
    And the answer's "/errors/0" is {"file": "App/Chat/qmldir", "line": 2, "message": "block Thread: App/Chat/Thread.qml does not exist"}

  Scenario: A block written in Rust comes from the project's type dump
    Given the file "ljostypes.json":
      """json
      {"format": 2, "modules": [{"uri": "App.Net", "versions": ["1.0"], "types": [
        {"name": "Feed", "since": "1.0", "base": null, "creatable": true,
         "defaultProperty": null, "attached": null,
         "properties": [{"name": "loading", "type": "bool", "readonly": true, "final": false,
           "required": false, "constant": false, "group": false, "notify": "loadingChanged"}],
         "signals": [], "methods": [{"name": "refresh", "parameters": [], "returnType": "void"}],
         "block": {"category": "Network"}}]}]}
      """
    When the agent calls the tool "blocks_list" with:
      """json
      {"module": "App.Net"}
      """
    Then the answer's "/blocks/0/origin" is {"kind": "rust"}
    And the answer's "/blocks/0/category" is "Network"
    And the answer's "/blocks/0/state/0/name" is "loading"
    And the answer's "/blocks/0/actions" is [{"name": "refresh", "params": [], "returns": null}]
