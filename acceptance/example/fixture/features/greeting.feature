Feature: Welcome screen
  The welcome screen greets the guest by the name on their badge.

  Scenario Outline: The headline names the guest
    Given a welcome screen for the badge "<badge>"
    Then the headline reads "Hello, <badge>!"

    Examples:
      | badge |
      | Ada   |
      | Grace |
