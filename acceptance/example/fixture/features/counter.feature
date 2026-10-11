Feature: Visit counter
  A kiosk screen counts the people who walk past. The visitor can add one
  or take one away.

  Scenario: A fresh counter starts at the number it was given
    Given a counter screen that starts at 3
    Then the counter shows 3

  Scenario: Adding one
    Given a counter screen that starts at 3
    When the visitor taps Add
    Then the counter shows 4

  Scenario: Taking away several
    Given a counter screen that starts at 3
    When the visitor taps Remove 2 times
    Then the counter shows 1

  Scenario: The counter has an Add and a Remove button
    Given a counter screen that starts at 0
    Then the screen offers the buttons Add and Remove
