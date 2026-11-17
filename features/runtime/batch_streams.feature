# Sources:
#   docs/ARCHITECTURE.md sections 3.3 (side streams), 3.4 and 19.4
#   https://www.erlang.org/doc/apps/observer/observer_ug.html
# Side streams are a Ljós mechanism: Qt has nothing to compare them with.

@ljos-only @headless
Feature: Side streams reach the UI thread in batches
  A thread that feeds the UI many small items (folder events, a child
  process's output, chunks of a transcript, the deltas of a stream) sends
  them into a side stream, not one message each. A stream holds a fixed
  number of items and never makes its sender wait: when it is full, its
  policy says what gives way. It wakes the UI thread when its first item
  arrives, and the UI thread hands everything queued to its handler in one
  turn. An observer lists each stream as a mailbox of kind "batch".

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item { width: 100; height: 100 }
      """
    And the inspector is connected

  Rule: A burst wakes the UI thread once and is handled in one turn

    Scenario: Ten thousand items wake once and are handled together
      Given a stream "burst" of 10000 items that refuses the newest when full
      When 10000 items are sent to the stream "burst"
      Then the UI thread was woken 1 time
      When the UI thread handles the stream "burst"
      Then the stream "burst" was handled in 1 turn of 10000 items
      And the stream "burst" holds 0 items

    Scenario: A stream nobody sends to wakes nothing
      Given a stream "quiet" of 8 items that refuses the newest when full
      When the UI thread handles the stream "quiet"
      And the UI thread handles the stream "quiet"
      Then the UI thread was woken 0 times
      And the stream "quiet" was handled in 0 turns

    Scenario: Items sent after a turn wake the UI thread again
      Given a stream "again" of 8 items that refuses the newest when full
      When 3 items are sent to the stream "again"
      And the UI thread handles the stream "again"
      And 2 items are sent to the stream "again"
      Then the UI thread was woken 2 times

  Rule: When the stream is full, its policy says what gives way

    Scenario: Coalescing keeps the newest item of each key
      Given a stream "progress" of 4 items that keeps the newest per key
      When the stream "progress" is sent "a=1, b=1, a=2, a=3, b=2"
      And the UI thread handles the stream "progress"
      Then the stream "progress" handed over "a=3, b=2"
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "mailbox", "mailbox": "progress"}
        """
      Then the answer's "/mailboxes/0/kind" is "batch"
      And the answer's "/mailboxes/0/sent" is 5
      And the answer's "/mailboxes/0/replaced" is 3
      And the answer's "/mailboxes/0/drained" is 2

    Scenario: Refusing the newest keeps the items already queued
      Given a stream "lines" of 3 items that refuses the newest when full
      When the stream "lines" is sent "one, two, three, four, five"
      And the UI thread handles the stream "lines"
      Then the stream "lines" handed over "one, two, three"
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "mailbox", "mailbox": "lines"}
        """
      Then the answer's "/mailboxes/0/sent" is 3
      And the answer's "/mailboxes/0/refused" is 2
      And the answer's "/mailboxes/0/evicted" is 0

    Scenario: Evicting the oldest keeps the newest items
      Given a stream "events" of 3 items that evicts the oldest when full
      When the stream "events" is sent "one, two, three, four, five"
      And the UI thread handles the stream "events"
      Then the stream "events" handed over "three, four, five"
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "mailbox", "mailbox": "events"}
        """
      Then the answer's "/mailboxes/0/sent" is 5
      And the answer's "/mailboxes/0/evicted" is 2
      And the answer's "/mailboxes/0/refused" is 0

  Rule: A stream left with items after its turn asks for another

    Scenario: A budget that stops short wakes the UI thread again
      Given a stream "chunks" of 16 items that refuses the newest when full
      When 10 items are sent to the stream "chunks"
      And the UI thread handles at most 4 items of the stream "chunks"
      Then the stream "chunks" holds 6 items
      And the UI thread was woken 2 times
      When the UI thread handles the stream "chunks"
      Then the stream "chunks" was handled in 2 turns of 10 items
      And the stream "chunks" holds 0 items
      And the UI thread was woken 2 times

  Rule: An observer lists each stream with its counts, never its items

    Scenario: The stream is listed as a mailbox of kind batch
      Given a stream "deltas" of 16 items that refuses the newest when full
      When 5 items are sent to the stream "deltas"
      And the agent calls the tool "app_processes" with:
        """json
        {"kind": "mailbox", "mailbox": "deltas"}
        """
      Then the answer's "/mailboxes" has 1 entries
      And the answer's "/mailboxes/0/kind" is "batch"
      And the answer's "/mailboxes/0/capacity" is 16
      And the answer's "/mailboxes/0/depth" is 5
      And the answer's "/mailboxes/0/sent" is 5
      And the answer's "/mailboxes/0/drained" is 0

    Scenario: The application's inbox is listed as a mailbox of kind inbox
      When the agent calls the tool "app_processes" with:
        """json
        {"kind": "mailbox", "mailbox": "inspector"}
        """
      Then the answer's "/mailboxes/0/kind" is "inbox"
      And the answer's "/mailboxes/0/evicted" is 0
