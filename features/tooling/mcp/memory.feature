# Sources:
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle) and section 19.4
#   (`app_metrics`, `memory`)
#   https://www.erlang.org/doc/apps/observer/observer_ug.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: An agent reads where the application's memory is
  `app_metrics` with `memory` breaks down what the application holds, as an observer's
  allocator view does: its objects by type, the script heap, the reactive
  graph, the text caches, and per window the scene's tree and display
  lists and what its renderer keeps (with the text runs it holds drawn
  without some of their glyphs, the glyph meshes it built and the last
  frame's warnings), beside the process's resident bytes.
  It is also broken down by owner, each with bytes and a count, and
  reconciled with what is resident: the program, the owners, what the
  allocator keeps free, and the rest, told as not attributed.
  Counts are exact. Bytes no allocator counts for us are estimates, and
  say so. Asking reads running counts and walks the objects once; nothing
  is kept up to date for it, so an application at rest stays at rest.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        width: 200; height: 200
        property var names: []
        Column {
          Repeater {
            model: 5
            Text { text: "row " + index }
          }
        }
        Timer { interval: 1000 }
        ListModel { ListElement { name: "a" } ListElement { name: "b" } }
        Component.onCompleted: names = ["a", "b", "c"].map(function (n) { return n + n })
      }
      """
    And the inspector is connected with the capabilities "inspect"

  Scenario: The objects are counted by type, the largest first
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {}}
      """
    Then the answer's "/objects/count" is at least 8
    And the answer's "/objects/bytes" is at least 1
    And the answer's "/objects/approximate" is true
    And the answer's "/objects/types/0/bytes" is at least 1
    And the answer's text contains "objects"

  Scenario: One type is asked for by its name
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {"type": "Text"}}
      """
    Then the answer's "/objects/types" has 1 entry
    And the answer's "/objects/types/0/type" is "Text"
    And the answer's "/objects/types/0/count" is 5

  Scenario: The list of types is as long as asked
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {"max": 2}}
      """
    Then the answer's "/objects/types" has 2 entries
    And the answer's "/objects/types_left_out" is at least 1

  Scenario: The script heap is the allocator's own count
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {}}
      """
    Then the answer's "/script/bytes" is at least 1
    And the answer's "/script/approximate" is false

  Scenario: The reactive graph, the text caches and the process
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {}}
      """
    Then the answer's "/reactive/nodes" is at least 1
    And the answer's "/reactive/bytes" is at least 1
    And the answer's "/text/layouts" is at least 1
    And the answer's "/text/bytes" is at least 1
    And the answer's "/process/resident_bytes" is at least 1
    And the answer's "/counted_bytes" is at least 1

  # Headless, nothing renders: the scene is there, a renderer is not.
  Scenario: Each window has its scene, and its renderer where one draws
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {}}
      """
    Then the answer's "/windows" has 1 entry
    And the answer's "/windows/0/scene/nodes" is at least 1
    And the answer's "/windows/0/scene/lists" is at least 1
    And the answer's "/windows/0/scene/bytes" is at least 1
    And the answer's "/windows/0/render" is null

  Scenario: Each owner says what it holds and how many of what
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {}}
      """
    Then the answer's "/owners/0/owner" is "script"
    And the answer's "/owners/0/bytes" is at least 1
    And the answer's "/owners/0/count" is at least 1
    And the answer's "/owners/0/of" is "objects"
    And the answer's "/owners/8/owner" is "models"
    And the answer's "/owners/8/count" is at least 2
    And the answer's "/owners/8/bytes" is at least 1
    And the answer's "/owners/9/owner" is "compiled"
    And the answer's "/owners/9/count" is at least 1
    And the answer's "/owners/9/bytes" is at least 1
    And the answer's text contains "by owner:"

  # Textures are told but not added: where there is a GPU they are there.
  Scenario: The owners reconcile with what is resident
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {}}
      """
    Then the answer's "/reconciled/resident_bytes" is at least 1
    And the answer's "/reconciled/attributed_bytes" is at least 1
    And the answer's "/reconciled/not_attributed_share" is at least 0
    And the answer's "/owners/11/owner" is "program"
    And the answer's "/owners/13/owner" is "not_attributed"
    And the answer's "/owners/14/owner" is "textures"
    And the answer's "/owners/14/gpu" is true
    And the answer's text contains "not attributed:"

  # A crate built on the engine tells of what it holds that no object
  # counts: the multimedia types of the pictures their players decoded and
  # their outputs show. Nothing plays here, so they hold none.
  Scenario: What the media types hold is under an owner of its own
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {}}
      """
    Then the answer's "/owners/10/owner" is "media"
    And the answer's "/owners/10/of" is "frames"
    And the answer's "/owners/10/count" is 0
    And the answer's "/owners/10/approximate" is false
    And the answer's text contains "decoded media"

  # The kernel's map of the process names each mapping's file; it is read
  # only when asked. Elsewhere than on Linux `known` is false.
  Scenario: The program and what is not attributed are split by mapping when asked
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {"mappings": true}}
      """
    Then the answer's "/mappings/known" is true
    And the answer's "/mappings/resident_bytes" is at least 1
    And the answer's "/mappings/groups/0/resident_bytes" is at least 1
    And the answer's "/mappings/program_split/0/bytes" is at least 1
    And the answer's "/mappings/not_attributed_split/0/bytes" is at least 1
    And the answer's text contains "program and libraries, by mapping:"
    And the answer's text contains "the program"
    And the answer's text contains "not attributed, by where it is:"

  Scenario: Without `mappings` the process's map is not read
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {}}
      """
    Then the answer's "/mappings" is null
    And the answer does not contain "by mapping"

  Scenario: Each object says about how much it holds
    When the agent calls the tool "app_objects"
    Then the answer's "/objects/0/bytes" is at least 1

  Scenario: A type that is not there is asked for
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {"type": "Slider"}}
      """
    Then the answer's "/objects/types" has 0 entries

  Scenario: Without `memory` the metrics are frame work, with a summary
    When the agent calls the tool "app_metrics"
    Then the answer's "/memory/objects" is at least 8
    And the answer's "/frames/count" is at least 0

  Scenario: The list of types is bounded
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {"max": 0}}
      """
    Then the agent is refused with "invalid_argument"
    And the refusal mentions "memory.max"

  # 20480 bytes is half the default budget: the answer is sent twice, as
  # data and as text, and each copy keeps to it. The types that do not
  # fit are counted with the ones past `max`.
  Scenario: A long list of types keeps to the answer's budget
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {"max": 1000}}
      """
    Then the answer is at most 20480 bytes
    And the answer's "/objects/types/0/bytes" is at least 1
