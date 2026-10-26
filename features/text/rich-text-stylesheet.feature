# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html#css-properties
#   https://doc.qt.io/qt-6/richtext-html-subset.html#supported-css-selectors
# Verified on Qt 6.11.2 (qml6, offscreen). A rule's colours and lines are
# looked at in pixels, as in rich-text-style.feature: an overline runs
# along the top of the line, and the text is spaces that do not break. What
# a rule does to the font is seen in the size of the text, the same as when
# the element's own style attribute says it.

@qt-parity @api-QtQuick.Text.textFormat
Feature: A style sheet in rich text
  A `<style>` element of RichText holds rules. A rule's selectors name an
  element (`p`), a class (`.name`), both (`p.name`), or several of those
  with commas between. What a rule says applies to each element it matches,
  as the element's own style attribute would, and before it: what the
  attribute says wins.

  Rule: Colours and lines

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "root"; width: 400; height: 400; color: "white"
          property string gap: "&nbsp;&nbsp;&nbsp;&nbsp;"
          component R: Text { textFormat: Text.RichText; font.pixelSize: 60; width: 200 }
          R { text: "<style>a{color:#e0115f;text-decoration:overline}</style><a href=\"x\">" + gap + "</a>" }
          R { y: 100; text: "<style>.mark{color:#2e8b57;text-decoration:overline}</style><span class=\"mark\">" + gap + "</span>" }
          R { y: 200; text: "<style>b,i{color:#00ff00;text-decoration:overline}</style><i>" + gap + "</i>" }
          R { y: 300; text: "<style>span{color:#00ff00;text-decoration:overline}</style><span style=\"color:#ff0000\">" + gap + "</span>" }
          R { x: 200; text: "<style>.c{color:#ff0000} span{color:#00ff00;text-decoration:overline}</style><span class=\"c\">" + gap + "</span>" }
          R { x: 200; y: 100; text: "<style>span{color:#ff0000;text-decoration:overline} span{color:#00ff00}</style><span>" + gap + "</span>" }
          R { x: 200; y: 200; text: "<style>span.c{color:#00ff00;text-decoration:overline} b.c{color:#ff0000}</style><span class=\"a c\">" + gap + "</span>" }
          R { x: 200; y: 300; text: "<style>b{color:#ff0000;text-decoration:overline}</style><span>" + gap + "</span>" }
        }
        """
      When 100 milliseconds pass

    Scenario: A rule for an element gives a link its colour and lines
      Then the pixel at 5, 1 of the window is "#e0115f"

    Scenario: A rule for a class applies to an element of that class
      Then the pixel at 5, 101 of the window is "#2e8b57"

    Scenario: A rule with a list of selectors applies to each of them
      Then the pixel at 5, 201 of the window is "#00ff00"

    Scenario: The element's own style wins over a rule
      Then the pixel at 5, 301 of the window is "#ff0000"

    Scenario: A rule for a class wins over a rule for the element
      Then the pixel at 205, 1 of the window is "#ff0000"

    Scenario: Of two rules alike the later wins
      Then the pixel at 205, 101 of the window is "#00ff00"

    Scenario: An element is of each class its class attribute names
      Then the pixel at 205, 201 of the window is "#00ff00"

    Scenario: A rule for another element does nothing
      Then the pixel at 205, 301 of the window is "#ffffff"

  Rule: Fonts

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"; width: 400; height: 400
          property bool same: sheet.implicitWidth === inline.implicitWidth
          property bool bigger: sheet.implicitWidth > plain.implicitWidth
          property bool before: early.implicitWidth === plain.implicitWidth
          Text { id: sheet; textFormat: Text.RichText; text: "<style>.big{font-size:24px;font-weight:600}</style><p class=\"big\">Hello</p>" }
          Text { id: inline; textFormat: Text.RichText; text: "<p style=\"font-size:24px;font-weight:600\">Hello</p>" }
          Text { id: plain; textFormat: Text.RichText; text: "<p>Hello</p>" }
          Text { id: early; textFormat: Text.RichText; text: "<p class=\"big\">Hello</p><style>.big{font-size:24px;font-weight:600}</style>" }
        }
        """

    Scenario: A rule's font is the font the style attribute would give
      Then the property "same" of "root" is true
      And the property "bigger" of "root" is true

    Scenario: A rule is not for what was written before its sheet
      Then the property "before" of "root" is true
