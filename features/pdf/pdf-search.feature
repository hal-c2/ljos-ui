# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfsearchmodel.html
#   https://doc.qt.io/qt-6/qpdfsearchmodel.html
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdflink.html
# Verified on Qt 6.11.2 (qml6, offscreen). "text.pdf" is the document of
# pdf-text.feature. Qt searches after the change that asked, not during it:
# 300 milliseconds is enough for this document on both runtimes. A result's
# context is up to 64 characters each side of it, a line break shown as "⏎".
# Its rectangles are in points from the top left of its page, each side on a
# whole point. A polygon's points are not readable from a script, only how
# many polygons there are.

@qt-parity
Feature: Searching a PDF document
  A PdfSearchModel finds every place its "searchString" is in its document,
  whatever the case, page by page. Each result is a row of its page, its
  place among the page's results, where it starts and the text around it.
  "currentResult" is the result the reader is on; setting it moves
  "currentPage" to that result's page, and it wraps past either end.

  Background:
    Given a PDF file "text.pdf" of objects:
      """
      << /Type /Catalog /Pages 2 0 R /Outlines 8 0 R
         /PageLabels << /Nums [0 << /S /r >> 1 << /S /D /P (A-) >>] >> >>
      << /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>
      << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 4 0 R
         /Resources << /Font << /F1 7 0 R >> >> /Annots [11 0 R 12 0 R] >>
      << >>
      stream
      BT /F1 20 Tf 10 160 Td (Hello World) Tj 0 -30 Td (Second line hello) Tj ET
      endstream
      << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 6 0 R
         /Resources << /Font << /F1 7 0 R >> >> >>
      << >>
      stream
      BT /F1 10 Tf 20 50 Td (Page two says hello too) Tj ET
      endstream
      << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>
      << /Type /Outlines /First 9 0 R /Last 10 0 R /Count 2 >>
      << /Title (First) /Parent 8 0 R /Next 10 0 R /Dest [3 0 R /XYZ 10 150 2] >>
      << /Title (Second) /Parent 8 0 R /Prev 9 0 R /Dest [5 0 R /XYZ 0 100 0] >>
      << /Type /Annot /Subtype /Link /Rect [10 155 120 180] /Border [0 0 0]
         /A << /S /URI /URI (https://example.com/) >> >>
      << /Type /Annot /Subtype /Link /Rect [10 125 200 150] /Border [0 0 0]
         /Dest [5 0 R /XYZ 20 60 0] >>
      """

  @api-QtQuick.Pdf.PdfSearchModel.searchString @api-QtQuick.Pdf.PdfSearchModel.count
  Scenario: Results come after the search was asked for
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfSearchModel { id: m; objectName: "m"; document: d; searchString: "hello" }
        property int atFirst: -1
        Component.onCompleted: atFirst = m.count
      }
      """
    When 300 milliseconds pass
    Then the property "atFirst" of "root" is 0
    And the property "count" of "m" is 3
    And the log has no warnings

  @api-QtQuick.Pdf.PdfSearchModel.searchString
  Scenario Outline: Searching for "<text>" finds it <count> times
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        PdfDocument { id: d; source: "text.pdf" }
        PdfSearchModel { objectName: "m"; document: d; searchString: "<text>" }
      }
      """
    When 300 milliseconds pass
    Then the property "count" of "m" is <count>
    And the log has no warnings

    Examples:
      | text       | count |
      | hello      | 3     |
      | HeLLo      | 3     |
      | says hello | 1     |
      | sayshello  | 0     |
      | nowhere    | 0     |

  @api-QtQuick.Pdf.PdfSearchModel.searchString
  Scenario: Each result is a row of where it is and what is around it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfSearchModel { id: m; document: d; searchString: "hello" }
        Instantiator {
          id: rows
          model: m
          delegate: QtObject {
            required property int page
            required property int indexOnPage
            required property point location
            required property string contextBefore
            required property string contextAfter
            readonly property var row: [page, indexOnPage, location.x, location.y,
                                        contextBefore, contextAfter]
          }
        }
        property var seen: []
        function read() { seen = Array.from({ length: rows.count }, (_, i) => rows.objectAt(i).row) }
      }
      """
    When 300 milliseconds pass
    And the method "read" of "root" is called
    Then the property "seen" of "root" is [[0, 0, 12, 26, "", " World⏎Second line hello"], [0, 1, 121, 56, "Hello World⏎Second line ", ""], [1, 0, 88, 43, "Page two says ", " too"]]
    And the log has no warnings

  @api-QtQuick.Pdf.PdfSearchModel.currentResult @api-QtQuick.Pdf.PdfSearchModel.currentPage
  Scenario Outline: Setting the current result to <set> makes it <result> on page <page>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfSearchModel { id: m; document: d; searchString: "hello" }
        property var seen: []
        function choose(n) {
          m.currentResult = n
          seen = [m.currentResult, m.currentPage]
        }
      }
      """
    When 300 milliseconds pass
    And the method "choose" of "root" is called with <set>
    Then the property "seen" of "root" is [<result>, <page>]
    And the log has no warnings

    Examples:
      | set | result | page |
      | 1   | 1      | 0    |
      | 2   | 2      | 1    |
      | 7   | 0      | 0    |
      | -1  | 2      | 1    |

  @api-QtQuick.Pdf.PdfSearchModel.currentPage @api-QtQuick.Pdf.PdfSearchModel.currentResult
  Scenario: Turning the page or searching for something else keeps the current result
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfSearchModel { id: m; document: d; searchString: "hello" }
        property var seen: []
        function turn() {
          m.currentResult = 1
          m.currentPage = 1
          var turned = [m.currentResult, m.currentPage, m.currentResultBoundingPolygons.length,
                        m.currentResultBoundingRect.width]
          m.searchString = "hello too"
          seen = turned.concat([m.currentResult, m.currentPage])
        }
      }
      """
    When 300 milliseconds pass
    And the method "turn" of "root" is called
    Then the property "seen" of "root" is [1, 1, 0, 0, 1, 1]
    And the log has no warnings

  @api-QtQuick.Pdf.PdfSearchModel.currentResultBoundingRect
  @api-QtQuick.Pdf.PdfSearchModel.currentResultBoundingPolygons
  @api-QtQuick.Pdf.PdfSearchModel.currentPageBoundingPolygons
  @api-QtQuick.Pdf.PdfSearchModel.boundingPolygonsOnPage
  Scenario: A result is drawn around where it is on its page
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfSearchModel { id: m; document: d; searchString: "hello" }
        property var seen: []
        function read() {
          var r = m.currentResultBoundingRect
          seen = [r.x, r.y, r.width, r.height, m.currentResultBoundingPolygons.length,
                  m.currentPageBoundingPolygons.length, m.boundingPolygonsOnPage(1).length]
        }
      }
      """
    When 300 milliseconds pass
    And the method "read" of "root" is called
    Then the property "seen" of "root" is [12, 26, 43, 14, 1, 2, 1]
    And the log has no warnings

  @api-QtQuick.Pdf.PdfSearchModel.currentResultLink @api-QtQuick.Pdf.pdfLink
  Scenario: The current result is a link to where it is
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfSearchModel { id: m; document: d; searchString: "hello" }
        property var seen: []
        function read() {
          var l = m.currentResultLink
          seen = [l.valid, l.page, l.location.x, l.location.y, l.zoom, l.contextBefore,
                  l.contextAfter, l.rectangles.length, l.toString()]
        }
      }
      """
    When 300 milliseconds pass
    And the method "read" of "root" is called
    Then the property "seen" of "root" is [true, 0, 12, 26, 0, "", " World⏎Second line hello", 1, ""]
    And the log has no warnings
