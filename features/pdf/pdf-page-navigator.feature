# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfpagenavigator.html
# Verified on Qt 6.11.2 (qmltestrunner, offscreen). The page says jumped takes
# a page, a location and a zoom, and jump a fourth argument; Qt 6.11's takes
# neither, so these scenarios count the signal and call jump with three. Qt
# does not tell of what clear changed, so that scenario reads after it.

@qt-parity
Feature: A history of where a reader has been in a document
  A PdfPageNavigator starts on the first page. "jump" adds a destination to go
  back from and forgets what could be gone forward to; "update" changes where
  the reader is now, as scrolling does, and adds nothing; "back" and "forward"
  move along the destinations. Only those that move say "jumped". Everything
  it says is read-only.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        property int jumps: 0
        PdfPageNavigator { id: n; onJumped: root.jumps++ }
        readonly property var now: [n.currentPage, n.currentLocation.x, n.currentZoom,
                                    n.backAvailable, n.forwardAvailable]
        function jump(page) { n.jump(page, Qt.point(page, page), page) }
        function update(page) { n.update(page, Qt.point(page, page), page) }
        function back() { n.back() }
        function forward() { n.forward() }
        function clear() {
          n.clear()
          cleared = [n.currentPage, n.currentLocation.x, n.currentZoom,
                     n.backAvailable, n.forwardAvailable]
        }
        property var cleared
        function write() { try { n.currentPage = 9 } catch (e) { refused = true } }
        property bool refused: false
      }
      """

  @api-QtQuick.Pdf.PdfPageNavigator.currentPage
  Scenario: A navigator starts on the first page, with nowhere to go
    Then the property "now" of "root" is [0, 0, 1, false, false]
    When the method "back" of "root" is called
    And the method "forward" of "root" is called
    Then the property "now" of "root" is [0, 0, 1, false, false]
    And the property "jumps" of "root" is 0

  @api-QtQuick.Pdf.PdfPageNavigator.jump @api-QtQuick.Pdf.PdfPageNavigator.jumped
  Scenario: A jump can be gone back from
    When the method "jump" of "root" is called with 2
    Then the property "now" of "root" is [2, 2, 2, true, false]
    And the property "jumps" of "root" is 1
    When the method "jump" of "root" is called with 3
    Then the property "now" of "root" is [3, 3, 3, true, false]
    When the method "back" of "root" is called
    Then the property "now" of "root" is [2, 2, 2, true, true]
    When the method "back" of "root" is called
    Then the property "now" of "root" is [0, 0, 1, false, true]
    And the property "jumps" of "root" is 4
    When the method "forward" of "root" is called
    Then the property "now" of "root" is [2, 2, 2, true, true]
    And the property "jumps" of "root" is 5

  @api-QtQuick.Pdf.PdfPageNavigator.jump
  Scenario: A jump forgets what could be gone forward to
    When the method "jump" of "root" is called with 2
    And the method "back" of "root" is called
    And the method "jump" of "root" is called with 5
    Then the property "now" of "root" is [5, 5, 5, true, false]
    When the method "forward" of "root" is called
    Then the property "now" of "root" is [5, 5, 5, true, false]
    When the method "back" of "root" is called
    Then the property "now" of "root" is [0, 0, 1, false, true]

  @api-QtQuick.Pdf.PdfPageNavigator.update
  Scenario: An update changes where the reader is, quietly
    When the method "jump" of "root" is called with 2
    And the method "jump" of "root" is called with 3
    And the method "back" of "root" is called
    Then the property "jumps" of "root" is 3
    When the method "update" of "root" is called with 7
    Then the property "now" of "root" is [7, 7, 7, true, true]
    And the property "jumps" of "root" is 3
    When the method "forward" of "root" is called
    Then the property "now" of "root" is [3, 3, 3, true, false]
    When the method "back" of "root" is called
    Then the property "now" of "root" is [7, 7, 7, true, true]

  @api-QtQuick.Pdf.PdfPageNavigator.jump
  Scenario: Jumping to where the reader is goes nowhere
    When the method "jump" of "root" is called with 5
    And the method "jump" of "root" is called with 5
    Then the property "jumps" of "root" is 1
    When the method "back" of "root" is called
    Then the property "now" of "root" is [0, 0, 1, false, true]

  @api-QtQuick.Pdf.PdfPageNavigator.clear
  Scenario: Clearing forgets everything
    When the method "jump" of "root" is called with 5
    And the method "clear" of "root" is called
    Then the property "cleared" of "root" is [0, 0, 1, false, false]
    When the method "back" of "root" is called
    Then the property "jumps" of "root" is 1

  @api-QtQuick.Pdf.PdfPageNavigator.currentPage
  Scenario: The current page is not written from outside
    When the method "write" of "root" is called
    Then the property "refused" of "root" is true
    And the property "now" of "root" is [0, 0, 1, false, false]
