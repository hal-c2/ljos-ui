# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdflinkmodel.html
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfbookmarkmodel.html
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdflink.html
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfpagenavigator.html
# Verified on Qt 6.11.2 (qml6, offscreen). "text.pdf" is the document of
# pdf-text.feature: its first page has a link to https://example.com/ and a
# link to 20, 60 of the second page, and its outline is "First" (10, 150 of
# the first page at zoom 2) and "Second" (0, 100 of the second at zoom 0).
# Locations are in points from the top left of the page they are on. A link
# whose destination gives no zoom is a zoom of 1; an outline entry's is 0.

@qt-parity
Feature: Links and the outline of a PDF document
  A PdfLinkModel lists the links of one page of its document: where each is
  on the page and where it goes, a web address or a place in the document. A
  PdfBookmarkModel is the document's outline, a tree of titled places. A
  PdfPageNavigator jumps to a link.

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

  @api-QtQuick.Pdf.PdfLinkModel.document @api-QtQuick.Pdf.PdfLinkModel.page
  Scenario: A link to a web address
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfLinkModel { id: m; document: d; page: 0 }
        Instantiator {
          id: rows
          model: m
          delegate: QtObject {
            required property var link
            required property rect rectangle
            required property url url
            required property int page
            required property point location
            required property real zoom
            readonly property var row: [rectangle.x, rectangle.y, rectangle.width,
              rectangle.height, String(url), page, location.x, location.y, zoom,
              link.valid, link.toString()]
          }
        }
        property var seen: []
        function read() { seen = [rows.count, rows.objectAt(0).row] }
      }
      """
    When the method "read" of "root" is called
    Then the property "seen" of "root" is [2, [10, 20, 110, 25, "https://example.com/", -1, 0, 0, 1, false, "https://example.com/"]]
    And the log has no warnings

  # Qt measures where a link goes with the height of the page the link is on,
  # not the page it goes to: 140 here, below the bottom of that page.
  @qt-diverges
  @api-QtQuick.Pdf.PdfLinkModel.page @api-QtQuick.Pdf.pdfLink
  Scenario: A link to a place in the document
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfLinkModel { id: m; document: d; page: 0 }
        Instantiator {
          id: rows
          model: m
          delegate: QtObject {
            required property var link
            required property rect rectangle
            required property url url
            required property int page
            required property point location
            required property real zoom
            readonly property var row: [rectangle.x, rectangle.y, rectangle.width,
              rectangle.height, String(url), page, location.x, location.y, zoom,
              link.valid, link.toString()]
          }
        }
        property var seen: []
        function read() { seen = rows.objectAt(1).row }
      }
      """
    When the method "read" of "root" is called
    Then the property "seen" of "root" is [10, 50, 190, 25, "", 1, 20, 40, 1, true, "Page 1 location 20.0, 40.0 zoom 1"]
    And the log has no warnings

  @api-QtQuick.Pdf.PdfLinkModel.page
  Scenario: Turning to a page lists that page's links
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfLinkModel { id: m; document: d; page: 0 }
        Instantiator { id: rows; model: m; delegate: QtObject {} }
        property var seen: []
        function read() { seen = [rows.count]; m.page = 1; seen.push(rows.count) }
      }
      """
    When the method "read" of "root" is called
    Then the property "seen" of "root" is [2, 0]
    And the log has no warnings

  # Qt 6.11 lists the links of the first page until the page is changed,
  # whatever page the model was given first.
  @qt-diverges
  @api-QtQuick.Pdf.PdfLinkModel.page
  Scenario: A page without links has no rows
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfLinkModel { id: m; document: d; page: 1 }
        Instantiator { id: rows; model: m; delegate: QtObject {} }
        property var seen: []
        function read() { seen = [rows.count] }
      }
      """
    When the method "read" of "root" is called
    Then the property "seen" of "root" is [0]
    And the log has no warnings

  @api-QtQuick.Pdf.PdfBookmarkModel.document
  Scenario: The outline is a row for each entry
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfBookmarkModel { id: m; document: d }
        Instantiator {
          id: rows
          model: m
          delegate: QtObject {
            required property string title
            required property int level
            required property int page
            required property point location
            required property real zoom
            readonly property var row: [title, level, page, location.x, location.y, zoom]
          }
        }
        property var seen: []
        function read() { seen = Array.from({ length: rows.count }, (_, i) => rows.objectAt(i).row) }
      }
      """
    When the method "read" of "root" is called
    Then the property "seen" of "root" is [["First", 0, 0, 10, 50, 2], ["Second", 0, 1, 0, 0, 0]]
    And the log has no warnings

  # Qt measures where the link goes with the height of the page it is on.
  @qt-diverges
  @api-QtQuick.Pdf.PdfPageNavigator.jump @api-QtQuick.Pdf.PdfPageNavigator.jumped
  Scenario: A navigator jumps to where a link goes
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfLinkModel { id: m; document: d; page: 0 }
        Instantiator { id: rows; model: m; delegate: QtObject { required property var link } }
        PdfPageNavigator {
          id: n
          onJumped: (current) => root.heard = [current.page, current.location.y]
        }
        property var heard: []
        property var seen: []
        function follow() {
          n.jump(rows.objectAt(1).link)
          seen = [n.currentPage, n.currentLocation.x, n.currentLocation.y, n.currentZoom,
                  n.backAvailable]
        }
      }
      """
    When the method "follow" of "root" is called
    Then the property "seen" of "root" is [1, 20, 40, 1, true]
    And the property "heard" of "root" is [1, 40]
    And the log has no warnings
