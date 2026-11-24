# Sources:
#   docs/DIALECT.md section 4.1 (Ljós's modules), 4.13 (translation, `t`), 4.16 (Ljos.Controls)
#   docs/site/README.md (the rules of a page's code blocks)
#   crates/ljos_cli/tests/docs_site.txt (the examples kept in Qt's QML, and why)
# `mise-tasks/docs/dialect` lints Ljós's own documentation: Qt has nothing to compare it with.
# The project stands in for a checkout: "site" is its docs/site and "pending.txt" its
# crates/ljos_cli/tests/docs_site.txt, the list of examples kept in Qt's QML for a gap.

@ljos-only @headless
Feature: The site's examples are written in Ljós's dialect
  An example on a page of the site is written as Ljós writes it: its types
  come from `Ljos.Core`, `Ljos.Ui`, `Ljos.Layout` and `Ljos.Models`, its
  controls from the styled `Ljos.Controls`, and its text is translated
  with `` t`…` ``. Qt's forms, `import QtQuick`, `qsTr` and
  `Ljos.Controls.Basic`, appear only where showing them is the point: on
  the page about porting from Qt, and in an example the list of pending
  examples keeps in Qt's QML for a gap that migrate cannot cross yet.
  `mise run docs:dialect` says which example strays and how to write it,
  so that the docs cannot drift back to Qt's forms.

  Background:
    Given the file "pending.txt":
      """
      # <page> <n> <reason>
      controls/sliders.md 2 gap:controls
      """

  Scenario: Examples in Ljós's dialect pass
    Given the file "site/controls/sliders.md":
      """
      ```qml
      import Ljos.Core
      import Ljos.Ui
      import Ljos.Controls
      Button { text: t`Save` }
      ```

      ```qml
      import QtQuick
      import QtQuick.Controls
      Dial { Accessible.name: qsTr("Volume") }
      ```
      """
    When the author runs "mise-tasks/docs/dialect site --pending pending.txt"
    Then the command exits with status 0
    And the command's output mentions "2 examples on 1 page"

  Scenario: An example that translates with qsTr is told to use t
    Given the file "site/start/first-app.md":
      """
      ```qml
      import Ljos.Ui
      import Ljos.Controls
      Button { text: qsTr("Save") }
      ```
      """
    When the author runs "mise-tasks/docs/dialect site --pending pending.txt"
    Then the command exits with status 1
    And the command's output mentions "site/start/first-app.md:1 (start/first-app.md#1): `qsTr` is Qt's; translate with t`…` from Ljos.Core"

  Scenario: A script beside an example translates with t too
    Given the file "site/data/tasks.md":
      """
      ```ts
      export function title(n: number): string {
          return qsTr("%1 tasks").arg(n)
      }
      ```
      """
    When the author runs "mise-tasks/docs/dialect site --pending pending.txt"
    Then the command exits with status 1
    And the command's output mentions "site/data/tasks.md:1: `qsTr` is Qt's; translate with t`…` from Ljos.Core"

  Scenario: A script of an example kept in Qt's QML for a gap is Qt's too
    Given the file "site/controls/sliders.md":
      """
      ```qml
      import Ljos.Ui
      Item {}
      ```

      ```qml group=volume
      import QtQuick
      import QtQuick.Controls
      Dial { Accessible.name: Labels.volume() }
      ```

      ```ts file=Labels.ts group=volume
      export function volume(): string { return qsTr("Volume") }
      ```
      """
    When the author runs "mise-tasks/docs/dialect site --pending pending.txt"
    Then the command exits with status 0

  Scenario: A script of an example in Ljós's dialect translates with t
    Given the file "site/controls/sliders.md":
      """
      ```qml group=volume
      import Ljos.Ui
      import Ljos.Controls
      Slider { Accessible.name: Labels.volume() }
      ```

      ```ts file=Labels.ts group=volume
      export function volume(): string { return qsTr("Volume") }
      ```
      """
    When the author runs "mise-tasks/docs/dialect site --pending pending.txt"
    Then the command exits with status 1
    And the command's output mentions "site/controls/sliders.md:7: `qsTr` is Qt's; translate with t`…` from Ljos.Core"

  Scenario: An example that imports QtQuick is told to import Ljós's modules
    Given the file "site/ui/items.md":
      """
      ```qml
      import QtQuick
      Rectangle { color: "teal" }
      ```
      """
    When the author runs "mise-tasks/docs/dialect site --pending pending.txt"
    Then the command exits with status 1
    And the command's output mentions "site/ui/items.md:1 (ui/items.md#1): `import QtQuick` is Qt's; import Ljos.Core, Ljos.Ui, Ljos.Layout or Ljos.Models, or list the example in the pending examples with the gap that keeps it"

  Scenario: An example that draws its controls in the Basic style is told to use Ljos.Controls
    Given the file "site/controls/buttons.md":
      """
      ```qml
      import Ljos.Ui
      import Ljos.Controls.Basic
      Button { text: "Save" }
      ```
      """
    When the author runs "mise-tasks/docs/dialect site --pending pending.txt"
    Then the command exits with status 1
    And the command's output mentions "site/controls/buttons.md:1 (controls/buttons.md#1): `import Ljos.Controls.Basic` draws the Basic style; import the styled Ljos.Controls"

  Scenario: An example kept in Qt's QML for a gap still draws its controls in Ljós's style
    Given the file "site/controls/sliders.md":
      """
      ```qml
      import Ljos.Ui
      Item {}
      ```

      ```qml
      import QtQuick
      import Ljos.Controls.Basic
      Dial {}
      ```
      """
    When the author runs "mise-tasks/docs/dialect site --pending pending.txt"
    Then the command exits with status 1
    And the command's output mentions "site/controls/sliders.md:6 (controls/sliders.md#2): `import Ljos.Controls.Basic` draws the Basic style; import the styled Ljos.Controls"

  Scenario: A block fenced with tildes is one nothing would check
    Given the file "site/ui/items.md":
      """
      ~~~qml
      import QtQuick
      Rectangle {}
      ~~~
      """
    When the author runs "mise-tasks/docs/dialect site --pending pending.txt"
    Then the command exits with status 1
    And the command's output mentions "site/ui/items.md:1: a `~~~` fence; the site fences its blocks with backticks"

  Scenario: A tilde fence shown inside a backtick block is that block's text
    Given the file "site/start/writing.md":
      """
      ````md
      ~~~qml
      Item {}
      ~~~
      ````
      """
    When the author runs "mise-tasks/docs/dialect site --pending pending.txt"
    Then the command exits with status 0

  Scenario: The page about porting from Qt shows Qt's forms beside Ljós's
    Given the file "site/language/porting-from-qt.md":
      """
      ```qml before
      import QtQuick
      import QtQuick.Controls.Basic
      Button { text: qsTr("Save") }
      ```

      ```qml
      import Ljos.Core
      import Ljos.Controls
      Button { text: t`Save` }
      ```
      """
    When the author runs "mise-tasks/docs/dialect site --pending pending.txt"
    Then the command exits with status 0

  Scenario: An example listed for a gap that no longer imports Qt is taken off the list
    Given the file "site/controls/sliders.md":
      """
      ```qml
      import Ljos.Ui
      Item {}
      ```

      ```qml
      import Ljos.Ui
      import Ljos.Controls
      Slider {}
      ```
      """
    When the author runs "mise-tasks/docs/dialect site --pending pending.txt"
    Then the command exits with status 1
    And the command's output mentions "pending.txt: controls/sliders.md 2 is listed as kept in Qt's QML, but it imports no Qt module; take it off the list"
