# Sources:
#   bench/inbox/README.md (what the inbox does, its pinned layout)
#   docs/BENCHMARKS.md (the applications the bench measures)
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle)
#   docs/DIALECT.md (`t` strings, `plural`, reduced motion)
# The application is written in the Ljós dialect: Qt has nothing to compare it with.

@ljos-only
Feature: The inbox bench application
  `bench/inbox` is a team's issue inbox, the application the bench drives
  in Ljós and in every toolkit it is compared with. Its layout is pinned in
  logical pixels, so these scenarios click where the session clicks. They
  give it twelve reports instead of the ten thousand its generator writes:
  eleven open, one closed, the newest first.

  Background:
    Given the files of the bench application "inbox"
    And a 96 by 96 image file "data/avatars/00.png" filled with "#3366aa"
    And the text file "data/issues.json":
      """json
      [
        {"id": 1, "key": "INB-1", "title": "Render crash on resize", "author": "Ada Okafor", "assignee": "Ada Okafor", "avatar": "avatars/00.png", "status": "open", "priority": 0, "tags": ["crash", "render"], "starred": false, "comments": 1, "created": "2026-11-01T09:00:00Z", "updated": "2026-11-27T11:00:00Z", "body": "Steps.\n\nExpected and actual."},
        {"id": 2, "key": "INB-2", "title": "Search is slow", "author": "Chen Wei", "assignee": "", "avatar": "avatars/00.png", "status": "open", "priority": 1, "tags": ["perf"], "starred": true, "comments": 2, "created": "2026-11-01T09:00:00Z", "updated": "2026-11-27T10:00:00Z", "body": "Steps.\n\nExpected and actual."},
        {"id": 3, "key": "INB-3", "title": "Dark theme contrast", "author": "Hana Suzuki", "assignee": "Ada Okafor", "avatar": "avatars/00.png", "status": "open", "priority": 2, "tags": ["a11y"], "starred": false, "comments": 3, "created": "2026-11-01T09:00:00Z", "updated": "2026-11-27T09:00:00Z", "body": "Steps.\n\nExpected and actual."},
        {"id": 4, "key": "INB-4", "title": "Crash when offline", "author": "Nia Mensah", "assignee": "", "avatar": "avatars/00.png", "status": "closed", "priority": 0, "tags": ["crash", "network"], "starred": false, "comments": 4, "created": "2026-11-01T09:00:00Z", "updated": "2026-11-27T08:00:00Z", "body": "Steps.\n\nExpected and actual."},
        {"id": 5, "key": "INB-5", "title": "Typo in settings 1", "author": "Chen Wei", "assignee": "", "avatar": "avatars/00.png", "status": "open", "priority": 3, "tags": ["text"], "starred": false, "comments": 5, "created": "2026-11-01T09:00:00Z", "updated": "2026-11-26T12:00:00Z", "body": "Steps.\n\nExpected and actual."},
        {"id": 6, "key": "INB-6", "title": "Typo in settings 2", "author": "Chen Wei", "assignee": "", "avatar": "avatars/00.png", "status": "open", "priority": 3, "tags": ["text"], "starred": false, "comments": 6, "created": "2026-11-01T09:00:00Z", "updated": "2026-11-25T12:00:00Z", "body": "Steps.\n\nExpected and actual."},
        {"id": 7, "key": "INB-7", "title": "Typo in settings 3", "author": "Chen Wei", "assignee": "", "avatar": "avatars/00.png", "status": "open", "priority": 3, "tags": ["text"], "starred": false, "comments": 7, "created": "2026-11-01T09:00:00Z", "updated": "2026-11-24T12:00:00Z", "body": "Steps.\n\nExpected and actual."},
        {"id": 8, "key": "INB-8", "title": "Typo in settings 4", "author": "Chen Wei", "assignee": "", "avatar": "avatars/00.png", "status": "open", "priority": 3, "tags": ["text"], "starred": false, "comments": 8, "created": "2026-11-01T09:00:00Z", "updated": "2026-11-23T12:00:00Z", "body": "Steps.\n\nExpected and actual."},
        {"id": 9, "key": "INB-9", "title": "Typo in settings 5", "author": "Chen Wei", "assignee": "", "avatar": "avatars/00.png", "status": "open", "priority": 3, "tags": ["text"], "starred": false, "comments": 9, "created": "2026-11-01T09:00:00Z", "updated": "2026-11-22T12:00:00Z", "body": "Steps.\n\nExpected and actual."},
        {"id": 10, "key": "INB-10", "title": "Typo in settings 6", "author": "Chen Wei", "assignee": "", "avatar": "avatars/00.png", "status": "open", "priority": 3, "tags": ["text"], "starred": false, "comments": 10, "created": "2026-11-01T09:00:00Z", "updated": "2026-11-21T12:00:00Z", "body": "Steps.\n\nExpected and actual."},
        {"id": 11, "key": "INB-11", "title": "Typo in settings 7", "author": "Chen Wei", "assignee": "", "avatar": "avatars/00.png", "status": "open", "priority": 3, "tags": ["text"], "starred": false, "comments": 11, "created": "2026-11-01T09:00:00Z", "updated": "2026-11-20T12:00:00Z", "body": "Steps.\n\nExpected and actual."},
        {"id": 12, "key": "INB-12", "title": "Typo in settings 8", "author": "Chen Wei", "assignee": "", "avatar": "avatars/00.png", "status": "open", "priority": 3, "tags": ["text"], "starred": false, "comments": 12, "created": "2026-11-01T09:00:00Z", "updated": "2026-11-19T12:00:00Z", "body": "Steps.\n\nExpected and actual."}
      ]
      """
    When "Main.qml" is loaded
    And the window is 1280 by 800 pixels
    And 1 second passes

  Rule: The list shows what the sidebar and the search choose

    Scenario: The inbox lists the open reports, the newest first
      Then the property "rows.length" of "store" is 11
      When the method "selectAt" of "store" is called with 0
      Then the property "selectedId" of "store" is 1
      And the accessibility tree has a "list" named "Issues"
      And the accessibility tree has a "label" named "11 issues"
      And the log has no warnings

    Scenario: The search narrows the list as the user types
      When the user touches 436, 28
      And the user types "crash"
      Then the property "rows.length" of "store" is 1
      When the user types " offline"
      Then the property "rows.length" of "store" is 0
      And the accessibility tree has a "label" named "No issues match"

    Scenario: Escape in the search field shows the whole list again
      When the user touches 436, 28
      And the user types "typo"
      And the user presses "Escape"
      Then the property "rows.length" of "store" is 11

    Scenario: Closed lists only the closed reports
      When the user clicks the "button" "Closed"
      Then the property "rows.length" of "store" is 1
      When the method "selectAt" of "store" is called with 0
      Then the property "selectedId" of "store" is 4

  Rule: The keyboard moves the selection, and the selection follows the report

    Background:
      When the user touches 460, 120

    Scenario: A click on a row selects its report
      Then the property "selectedId" of "store" is 1

    Scenario: Page Down moves the selection ten rows
      When the user presses "PageDown"
      Then the property "selectedIndex" of "store" is 10
      When the user presses "PageUp"
      Then the property "selectedIndex" of "store" is 0

    Scenario: Another sort keeps the same report selected where it now stands
      When the user presses "Down"
      And the user presses "Down"
      And the property "sort" of "store" is set to "title"
      Then the property "selectedId" of "store" is 3
      And the property "selectedIndex" of "store" is 0

  Rule: The edit form refuses bad input and saving moves the report to the top

    Background:
      When the user touches 460, 248
      And the user touches 940, 78
      And 300 milliseconds pass

    Scenario: An empty title turns Save off and says why
      When the user touches 990, 160
      And the user presses "Ctrl+A"
      And the user presses "Backspace"
      Then the property "enabled" of "save" is false
      And the accessibility tree has a "label" named "Title is required"
      When the user types "Contrast"
      Then the property "enabled" of "save" is true

    Scenario: Saving shows the report again, at the top of the list
      When the user touches 990, 160
      And the user presses "End"
      And the user types " again"
      And the user touches 1196, 660
      Then the property "tab" of "store" is 0
      And the property "selected.title" of "store" is "Dark theme contrast again"
      And the property "selectedIndex" of "store" is 0

  Rule: Closing and creating reports

    Scenario: Closing a report takes it out of the inbox and selects the next
      When the user touches 460, 120
      And 300 milliseconds pass
      And the user touches 1196, 132
      Then the property "rows.length" of "store" is 10
      And the property "selectedId" of "store" is 2

    Scenario: A new report is selected at the top, its form open
      When the user touches 888, 28
      Then the property "selectedId" of "store" is 13
      And the property "selectedIndex" of "store" is 0
      And the property "tab" of "store" is 1

  Rule: It speaks Japanese and rests when nothing happens

    Scenario: In Japanese the count is counted in 件
      When the UI language is set to "ja"
      Then the accessibility tree has a "label" named "11 件"
      And the accessibility tree has a "button" named "新しい課題"

    Scenario: With the reports shown and nothing happening, the application rests
      Given the inspector is connected
      When 1 second passes
      Then the application is left idle
