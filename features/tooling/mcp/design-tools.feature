# Sources:
#   docs/ARCHITECTURE.md sections 14 (design guardrails) and 19.3
#   https://www.w3.org/TR/WCAG22/#contrast-minimum
#   https://www.w3.org/TR/WCAG22/#non-text-contrast
#   https://doc.qt.io/qt-6/qml-qtquick-propertyanimation.html#easing-prop
#   https://doc.qt.io/qt-6/qml-qtquick-springanimation.html
# The authoring tools are a Ljós extension: Qt has nothing to compare them with.

@ljos-only @headless
Feature: Choosing colours and motion with the design tools
  An agent that needs a colour, a duration or an easing asks for one
  instead of guessing: `design_palette` grows a palette from a seed,
  `design_contrast` measures a pair as the scene blends it,
  `design_motion` gives a duration, a curve and the QML for a kind of
  motion, and `design_rules` lists what every check holds an interface to.
  The answers read the same table as `ljos check` and `ljos audit`, so
  what a tool suggests passes the checks.

  Background:
    Given the authoring tools are connected

  Rule: The rules and their limits are one table

    Scenario: A rule says what is wrong, how to fix it and where it is explained
      When the agent calls the tool "design_rules" with:
        """json
        {"code": "motion-ease-in-enter"}
        """
      Then the answer's "/rules" has 1 entries
      And the answer's "/rules/0/area" is "motion"
      And the answer's "/rules/0/guide" is "docs/design/motion.md#motion-ease-in-enter"
      And the answer's "/limits/0/name" is "text_contrast"
      And the answer's "/limits/0/value" is 4.5

    Scenario: A code no rule has is not found
      When the agent calls the tool "design_rules" with:
        """json
        {"code": "color-rainbow"}
        """
      Then the agent is refused with "not_found"

  Rule: Motion has a duration within its limit and a curve that does not start slow

    Scenario: A panel entrance is a quarter of a second and eases out
      When the agent calls the tool "design_motion" with:
        """json
        {"kind": "enter", "size": "panel", "frequency": "occasional", "properties": "opacity"}
        """
      Then the answer's "/tokens/0/duration_ms" is 250
      And the answer's "/tokens/0/limit_ms" is 350
      And the answer's "/tokens/0/easing" is "easeOut"
      And the answer's "/tokens/0/qml" matches "easing\.bezierCurve: \[0\.23, 1, 0\.32, 1, 1, 1\]"
      And the answer's "/tokens/0/reduced/property" is "opacity"

    Scenario: The way out is quicker than the way in
      When the agent calls the tool "design_motion" with:
        """json
        {"kind": "exit", "size": "panel", "frequency": "occasional"}
        """
      Then the answer's "/tokens/0/duration_ms" is 200

    Scenario: Something met many times a day is nearly instant
      When the agent calls the tool "design_motion" with:
        """json
        {"kind": "move", "size": "small", "frequency": "frequent"}
        """
      Then the answer's "/tokens/0/limit_ms" is 100

    Scenario: A gesture follows a damped spring
      When the agent calls the tool "design_motion" with:
        """json
        {"kind": "gesture", "size": "small", "frequency": "occasional"}
        """
      Then the answer's "/tokens/0/qml" matches "^SpringAnimation \{"
      And the answer's "/tokens/0/spring/damping_ratio" is at least 0.7

  Rule: A palette passes every pair it promises

    Scenario: Text, fills and focus rings pass in a dark, high-contrast palette
      When the agent calls the tool "design_palette" with:
        """json
        {"seed": "#1d4ed8", "scheme": "dark", "contrast": "high"}
        """
      Then the answer's "/failing" is 0
      And the answer's "/roles/0/name" is "canvas"
      And the answer's "/roles/13/name" is "text"
      And the answer's "/checks/0/required" is 7
      And the answer's "/ramps/0/steps" has 12 entries
      And the answer's "/harmonies" has 4 entries

    Scenario: A seed that is not a colour or a hue is refused
      When the agent calls the tool "design_palette" with:
        """json
        {"seed": "bright"}
        """
      Then the agent is refused with "invalid_argument"

  Rule: Contrast is measured on what the eye gets

    Scenario: Grey text on a translucent dark card fails, and a passing grey is offered
      When the agent calls the tool "design_contrast" with:
        """json
        {"foreground": "#999999", "background": "#ffffff", "layers": [{"color": "#000000", "opacity": 0.5}]}
        """
      Then the answer's "/background" is "#808080"
      And the answer's "/passes" is false
      And the answer's "/rule" is "color-text-contrast"
      And the answer's "/nearest_passing" matches "^#[0-9a-f]{6}$"

    Scenario: A border needs less than text
      When the agent calls the tool "design_contrast" with:
        """json
        {"foreground": "#888888", "background": "#ffffff", "kind": "nontext"}
        """
      Then the answer's "/required" is 3
      And the answer's "/passes" is true
