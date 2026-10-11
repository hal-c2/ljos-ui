Feature: Things the kiosk will do later
  The suite's authors wrote the first ahead of the feature, and the second
  is a limit the kiosk does not have yet.

  @backlog-ljos
  Scenario: Turning the dial
    Given a counter screen that starts at 3
    When the visitor drags the dial to 50
    Then the counter shows 50

  Scenario: Counting past a hundred
    Given a counter screen that starts at 99
    When the visitor taps Add
    Then the counter shows 1000
