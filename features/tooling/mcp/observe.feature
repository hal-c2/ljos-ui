# Sources:
#   docs/ARCHITECTURE.md sections 3.3, 3.4, 19.4 and 19.5
#   https://www.erlang.org/doc/apps/observer/observer_ug.html
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @headless
Feature: Processes and mailboxes from the inspector
  Like Erlang's observer, an agent or the `ljos observe` window can see
  what does work in the application and, with it, the bounded mailboxes
  work reaches the UI thread through, without ever seeing a message's
  contents.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: top
        width: 200; height: 200
        Timer { id: tick; interval: 250; repeat: true; running: true }
        Timer { id: later; interval: 5000; running: false }
        Rectangle {
          width: 10; height: 10
          NumberAnimation on x { id: slide; from: 0; to: 100; duration: 1000; loops: Animation.Infinite }
        }
      }
      """
    And the inspector is connected

  Rule: The processes are the threads, the Timers and the animations that run

    Scenario: A running Timer is listed with its interval, a stopped one is not
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "timer"}
        """
      Then the answer's "/processes" has 1 entries
      And the answer's "/processes/0/name" is "Timer#tick"
      And the answer's "/processes/0/locator" is "id=tick"
      And the answer's "/processes/0/interval_ms" is 250
      And the answer's "/processes/0/repeat" is true
      And the answer's "/processes/0/state" is "waiting"

    Scenario: A running animation is listed
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "animation"}
        """
      Then the answer's "/processes" has 1 entries
      And the answer's "/processes/0/name" is "NumberAnimation#slide"
      And the answer's "/processes/0/state" is "running"

    Scenario: A kind nobody has is refused
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "fiber"}
        """
      Then the agent is refused with "invalid_argument"

  Rule: A mailbox shows how full it is, never what it holds

    # As Erlang's observer shows each process's message queue with it.
    Scenario: The mailboxes are listed with the processes
      When the agent calls the tool "app_processes"
      Then the answer's "/processes/0/kind" is "thread"
      And the answer's "/mailboxes/0/name" matches "^[a-z]+$"
      And the answer's "/counted_since" is "inspector attached"

    Scenario: The inspector's own inbox is listed with its capacity
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "mailbox", "mailbox": "inspector"}
        """
      Then the answer's "/mailboxes" has 1 entries
      And the answer's "/mailboxes/0/name" is "inspector"
      And the answer's "/mailboxes/0/capacity" is 16
      And the answer's "/processes" has 0 entries

    Scenario: Another kind lists no mailboxes
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "timer"}
        """
      Then the answer's "/mailboxes" is absent

    Scenario: A peek past the front twenty messages is refused
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "mailbox", "peek": 21}
        """
      Then the agent is refused with "invalid_argument"

  Rule: A process row names its object by a handle any tool takes

    Scenario: A Timer is listed with the handle of its object
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "timer"}
        """
      Then the answer's "/processes/0/handle" matches "^@[0-9]+\.[0-9]+$"

    Scenario: The process as a whole is listed with its memory
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "thread"}
        """
      Then the answer's "/process/threads" is at least 1
      And the answer's "/process/rss_bytes" is at least 1

  Rule: The object tree has every object, items without a role and objects that are not items

    Scenario: Timers and animations are in the tree beside the items
      When the agent calls the tool "app_objects" with:
        """json
        {"root": "id=top"}
        """
      Then the answer's "/objects" has 5 entries
      And the answer's "/objects/0/id" is "top"
      And the answer's "/objects/0/depth" is 0
      And the answer's "/objects/1/type" is "Timer"
      And the answer's "/objects/1/id" is "tick"
      And the answer's "/objects/1/depth" is 1
      And the answer's "/objects/1/item" is false
      And the answer's "/objects/4/type" is "NumberAnimation"
      And the answer's "/objects/4/depth" is 2

    Scenario: Each object has a handle
      When the agent calls the tool "app_objects" with:
        """json
        {"root": "id=top"}
        """
      Then the answer's "/objects/3/handle" matches "^@[0-9]+\.[0-9]+$"

    Scenario: A depth limit leaves out what is below it and says how much
      When the agent calls the tool "app_objects" with:
        """json
        {"root": "id=top", "depth": 1}
        """
      Then the answer's "/objects" has 4 entries
      And the answer's "/objects/3/type" is "Rectangle"
      And the answer's "/objects/3/hidden" is 1

    Scenario: A tree longer than one answer is cut where the next answer goes on
      When the agent calls the tool "app_objects" with:
        """json
        {"root": "id=top", "max": 3}
        """
      Then the answer's "/objects" has 3 entries
      And the answer's "/truncated" is true
      And the answer's "/next_cursor" matches "^@[0-9.]+>@[0-9.]+$"

    Scenario: The next part of a tree goes on at the cursor, at the depth it has
      When the agent calls the tool "app_objects" with:
        """json
        {"root": "id=top", "max": 3}
        """
      And the agent calls the tool "app_objects" with the answer's "/next_cursor" as "cursor"
      Then the answer's "/objects" has 2 entries
      And the answer's "/objects/0/type" is "Rectangle"
      And the answer's "/objects/0/depth" is 1
      And the answer's "/objects/1/type" is "NumberAnimation"
      And the answer's "/truncated" is false
      And the answer's "/next_cursor" is absent

    Scenario: A cursor given with another root is refused
      When the agent calls the tool "app_objects" with:
        """json
        {"root": "id=top", "max": 3}
        """
      And the agent calls the tool "app_objects" with:
        """json
        {"root": "id=top", "cursor": "@1.1>@2.1"}
        """
      Then the agent is refused with "invalid_argument"

    Scenario: A handle that is not one is refused
      When the agent calls the tool "app_inspect" with:
        """json
        {"target": "@twelve"}
        """
      Then the agent is refused with "invalid_argument"

    Scenario: A handle of an object that is gone is stale
      When the agent calls the tool "app_inspect" with:
        """json
        {"target": "@99999.1"}
        """
      Then the agent is told the reference is stale

  Rule: A watch answers when something changed and sleeps until then

    Scenario: A watch without counts answers the counts at once
      When the agent calls the tool "app_watch" with:
        """json
        {}
        """
      Then the answer's "/waited" is false
      And the answer's "/changed" has 0 entries
      And the answer's "/counts/reloads" is 0

    Scenario: A watch from counts that are not the inspector's answers at once with what differs
      When the agent calls the tool "app_watch" with:
        """json
        {"since": {"reloads": 7}}
        """
      Then the answer's "/waited" is false
      And the answer's "/changed/0" is "reloads"

    Scenario: A watch of an application that is changing all the time still gathers a burst
      When the agent calls the tool "app_watch" with:
        """json
        {"since": {"reloads": 7}, "settle_ms": 20}
        """
      Then the answer's "/waited" is false
      And the answer's "/settled" is true
      And the answer's "/changed/0" is "reloads"

    Scenario: A watch for what does not change waits out its time
      When the agent calls the tool "app_watch" with:
        """json
        {"since": {"reloads": 0}, "topics": ["reloads"], "wait_ms": 50}
        """
      Then the answer's "/waited" is true
      And the answer's "/changed" has 0 entries

    Scenario: A watch of more than thirty seconds is refused
      When the agent calls the tool "app_watch" with:
        """json
        {"wait_ms": 30001}
        """
      Then the agent is refused with "invalid_argument"

    Scenario: A topic nobody counts is refused
      When the agent calls the tool "app_watch" with:
        """json
        {"topics": ["colours"]}
        """
      Then the agent is refused with "invalid_argument"

  # A pass is drawn only in a window, so how the counts move is checked in
  # ljos_dev's tests; here, what a watch of items is told and refused.
  Rule: A watch can follow how often each of a few items was drawn again

    Scenario: A watched item is counted from when it is first watched
      When the agent calls the tool "app_watch" with:
        """json
        {"items": ["@1.1"]}
        """
      Then the answer's "/counts/items/@1.1" is 0

    Scenario: A watch for an item that is not drawn again waits out its time
      When the agent calls the tool "app_watch" with:
        """json
        {"items": ["@1.1"], "since": {"items": {"@1.1": 0}}, "topics": ["items"], "wait_ms": 50}
        """
      Then the answer's "/waited" is true
      And the answer's "/changed" has 0 entries

    Scenario: A watch of more than sixteen items is refused
      When the agent calls the tool "app_watch" with:
        """json
        {"items": ["@1.1", "@2.1", "@3.1", "@4.1", "@5.1", "@6.1", "@7.1", "@8.1", "@9.1",
                   "@10.1", "@11.1", "@12.1", "@13.1", "@14.1", "@15.1", "@16.1", "@17.1"]}
        """
      Then the agent is refused with "invalid_argument"

    Scenario: An item that is not named by a handle is refused
      When the agent calls the tool "app_watch" with:
        """json
        {"items": ["id=top"]}
        """
      Then the agent is refused with "invalid_argument"
