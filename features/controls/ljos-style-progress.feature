# Sources:
#   docs/DIALECT.md (§4.16 the controls)
#   docs/design/color.md, docs/design/motion.md
#   https://doc.qt.io/qt-6/qml-qtquick-controls-progressbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-busyindicator.html
#   https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html
# Ljós's own style is Ljós's: Qt has no Ljos.Controls to compare it with, so
# every scenario is `@ljos-only`. The colours expected are those
# crates/ljos_design/src/roles.rs derives from white, black and #0066cc.

@ljos-only
Feature: Ljós's own style: how far a task is and that it is still working
  A progress bar and a busy indicator of Ljos.Controls are drawn in Ljós's
  own style: a thin rounded track filled in the focus colour, and the
  style's loading icon turning. When how far is not known, the bar sweeps
  a short fill along its track. Under reduced motion neither moves
  across: both pulse in place, so the wait still shows.

  Rule: A progress bar fills its track as far as the task has come

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 240; height: 100
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#000000"; Theme.accent = "#0066cc" }
          ProgressBar { objectName: "copy"; width: 200; value: 0.5 }
          ProgressBar { objectName: "wait"; y: 40; width: 200; indeterminate: true }
        }
        """

    Scenario: The fill is in the focus colour on a track in the edge's colour
      Then the property "background.color" of "copy" is "#d6d6d6"
      And the property "background.height" of "copy" is 4
      And the property "contentItem.children.0.width" of "copy" is 100
      And the property "contentItem.children.0.color" of "copy" is "#0066cc"
      When the property "value" of "copy" is set to 1
      Then the property "contentItem.children.0.width" of "copy" is 200

    Scenario: Not knowing how far, a short fill sweeps along the track
      Then the property "contentItem.children.0.visible" of "wait" is false
      And the property "contentItem.children.1.visible" of "wait" is true
      And the property "contentItem.children.1.x" of "wait" is at most 0
      When 600 milliseconds pass
      Then the property "contentItem.children.1.x" of "wait" is at least 20

    Scenario: With reduced motion the fill stays at the start and pulses
      Given the system setting "reduced motion" is true
      When 400 milliseconds pass
      Then the property "contentItem.children.1.x" of "wait" is 0
      And the property "contentItem.children.1.opacity" of "wait" is at most 0.9

  Rule: A busy indicator turns the style's loading icon while it runs

    Background:
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 200; height: 100
          Component.onCompleted: { Theme.canvas = "#ffffff"; Theme.text = "#000000"; Theme.accent = "#0066cc" }
          BusyIndicator { objectName: "busy" }
        }
        """

    Scenario: Running, the icon is muted and turns
      Then the property "contentItem.children.0.color" of "busy" is "#707070"
      When 400 milliseconds pass
      Then the property "contentItem.children.0.rotation" of "busy" is at least 1

    Scenario: Stopped, it fades away
      When the property "running" of "busy" is set to false
      And 200 milliseconds pass
      Then the property "contentItem.opacity" of "busy" is 0
      And the property "contentItem.visible" of "busy" is false

    Scenario: With reduced motion the icon pulses instead of turning
      Given the system setting "reduced motion" is true
      When 400 milliseconds pass
      Then the property "contentItem.children.0.rotation" of "busy" is 0
      And the property "contentItem.children.0.opacity" of "busy" is at most 0.9

  Rule: Both pass the design audit

    Scenario Outline: A page that waits in <scheme> has nothing to report
      Given the system setting "color scheme" is "<scheme>"
      And the QML:
        """qml
        import Ljos.Ui
        import Ljos.Layout
        import Ljos.Controls
        Page {
          width: 400; height: 400
          title: "Upload"
          Column {
            x: 12; y: 12; spacing: 12
            Label { text: "Sending two files" }
            ProgressBar { width: 360; value: 0.4 }
            ProgressBar { width: 360; indeterminate: true }
            BusyIndicator { }
          }
        }
        """
      Then the design audit finds nothing

      Examples:
        | scheme |
        | light  |
        | dark   |
