# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfdocument.html
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfselection.html
#   https://doc.qt.io/qt-6/qpdfselection.html
#   https://doc.qt.io/qt-6/qpdfdocument.html#PageModelRole-enum
# Verified on Qt 6.11.2 (qml6, offscreen). "text.pdf" has two pages: 300 by
# 200 points with "Hello World" and "Second line hello" in Helvetica 20, and
# 200 by 100 with "Page two says hello too" in Helvetica 10. Its first page is
# labelled "i", its second "A-1". A selection's "from" and "to" are in pixels
# of the page drawn at "renderScale"; at 1 they are points from the top left.
# Qt 6.11 throws "Unknown method return type: QPdfSelection" from
# getSelection, getSelectionAtIndex and getAllText; Ljós answers them.

@qt-parity
Feature: The text of a PDF document
  A PdfDocument says what each page is labelled, from the document's own
  labels or its page numbers, and lists its pages as a model of labels and
  sizes. A PdfSelection is the text between two points on a page: the
  character at the first is in it, the one at the last only when the point is
  past that character's middle. Lines are joined by a carriage return and a
  line feed, and a gap between words is a space.

  Background:
    Given a PDF file "text.pdf" of objects:
      """
      << /Type /Catalog /Pages 2 0 R /Outlines 8 0 R
         /PageLabels << /Nums [0 << /S /r >> 1 << /S /D /P (A-) >>] >> >>
      endobj
      << /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>
      endobj
      << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 4 0 R
         /Resources << /Font << /F1 7 0 R >> >> /Annots [11 0 R 12 0 R] >>
      endobj
      << >>
      stream
      BT /F1 20 Tf 10 160 Td (Hello World) Tj 0 -30 Td (Second line hello) Tj ET
      endstream
      endobj
      << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 6 0 R
         /Resources << /Font << /F1 7 0 R >> >> >>
      endobj
      << >>
      stream
      BT /F1 10 Tf 20 50 Td (Page two says hello too) Tj ET
      endstream
      endobj
      << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>
      endobj
      << /Type /Outlines /First 9 0 R /Last 10 0 R /Count 2 >>
      endobj
      << /Title (First) /Parent 8 0 R /Next 10 0 R /Dest [3 0 R /XYZ 10 150 2] >>
      endobj
      << /Title (Second) /Parent 8 0 R /Prev 9 0 R /Dest [5 0 R /XYZ 0 100 0] >>
      endobj
      << /Type /Annot /Subtype /Link /Rect [10 155 120 180] /Border [0 0 0]
         /A << /S /URI /URI (https://example.com/) >> >>
      endobj
      << /Type /Annot /Subtype /Link /Rect [10 125 200 150] /Border [0 0 0]
         /Dest [5 0 R /XYZ 20 60 0] >>
      endobj
      """

  @api-QtQuick.Pdf.PdfDocument.pageLabel @api-QtQuick.Pdf.PdfDocument.pageIndexForLabel
  Scenario: A page is called what the document labels it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        readonly property var labels: d.status === PdfDocument.Ready
          ? [d.pageLabel(0), d.pageLabel(1), d.pageLabel(5)] : []
        readonly property var pages: d.status === PdfDocument.Ready
          ? [d.pageIndexForLabel("i"), d.pageIndexForLabel("A-1"),
             d.pageIndexForLabel("1"), d.pageIndexForLabel("nowhere")] : []
      }
      """
    Then the property "labels" of "root" is ["i", "A-1", "6"]
    And the property "pages" of "root" is [0, 1, -1, -1]
    And the log has no warnings

  @api-QtQuick.Pdf.PdfDocument.pageLabel @api-QtQuick.Pdf.PdfDocument.pageIndexForLabel
  Scenario: A document that labels nothing calls its pages by number
    Given a PDF file "pages.pdf" of pages "300 by 200, 100 by 50, 150 by 400" painted "#ff0000"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "pages.pdf" }
        readonly property var labels: d.status === PdfDocument.Ready
          ? [d.pageLabel(0), d.pageLabel(2), d.pageLabel(7)] : []
        readonly property var pages: d.status === PdfDocument.Ready
          ? [d.pageIndexForLabel("1"), d.pageIndexForLabel("3"), d.pageIndexForLabel("4")] : []
      }
      """
    Then the property "labels" of "root" is ["1", "3", "8"]
    And the property "pages" of "root" is [0, 2, -1]
    And the log has no warnings

  @api-QtQuick.Pdf.PdfDocument.pageModel
  Scenario: The page model has a row for each page, of its label and size
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        Instantiator {
          id: rows
          model: d.pageModel
          delegate: QtObject {
            required property string label
            required property size pointSize
            readonly property var row: [label, pointSize.width, pointSize.height]
          }
        }
        property var seen: []
        function read() {
          var all = []
          for (var i = 0; i < rows.count; ++i)
            all.push(rows.objectAt(i).row)
          seen = all
        }
      }
      """
    When the method "read" of "root" is called
    Then the property "seen" of "root" is [["i", 300, 200], ["A-1", 200, 100]]
    And the log has no warnings

  @api-QtQuick.Pdf.PdfSelection.from @api-QtQuick.Pdf.PdfSelection.to @api-QtQuick.Pdf.PdfSelection.text
  Scenario Outline: A selection from <from> to <to> is <text>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        width: 300; height: 200
        PdfDocument { id: d; source: "text.pdf" }
        PdfSelection {
          id: s
          objectName: "s"
          document: d
          page: 0
          from: Qt.point(<from>)
          to: Qt.point(<to>)
        }
      }
      """
    Then the property "text" of "s" is <text>
    And the log has no warnings

    Examples:
      | from    | to        | text                     |
      | 10, 30  | 70, 30    | "Hello "                 |
      | 10, 30  | 71, 30    | "Hello W"                |
      | 18, 30  | 30, 30    | "H"                      |
      | 30, 30  | 18, 30    | "H"                      |
      | 5, 30   | 30, 30    | "H"                      |
      | 12, 10  | 30, 30    | ""                       |
      | 30, 30  | 30, 30    | ""                       |
      | 56, 30  | 64, 30    | "o "                     |
      | 10, 30  | 1000, 1000 | ""                      |
      | 70, 60  | 20, 30    | "Hello World\\r\\nSecon" |

  @api-QtQuick.Pdf.PdfSelection.renderScale
  Scenario: A selection's points are pixels of the page at its scale
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfSelection {
          id: s
          objectName: "s"
          document: d
          page: 0
          renderScale: 2
          from: Qt.point(20, 60)
          to: Qt.point(142, 60)
        }
      }
      """
    Then the property "text" of "s" is "Hello W"
    And the log has no warnings

  @api-QtQuick.Pdf.PdfSelection.selectAll @api-QtQuick.Pdf.PdfSelection.geometry
  Scenario Outline: Selecting all of page <page> selects its text
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfSelection { id: s; document: d; page: <page> }
        property var seen: []
        function select() {
          s.selectAll()
          seen = [s.text, s.geometry.length]
        }
      }
      """
    When the method "select" of "root" is called
    Then the property "seen" of "root" is <seen>
    And the log has no warnings

    Examples:
      | page | seen                                      |
      | 0    | ["Hello World\\r\\nSecond line hello", 2] |
      | 1    | ["Page two says hello too", 1]            |

  @api-QtQuick.Pdf.PdfSelection.hold
  Scenario: A held selection stays while its points move
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfSelection {
          id: s
          objectName: "s"
          document: d
          page: 0
          from: Qt.point(10, 30)
          to: Qt.point(70, 30)
        }
        property var seen: []
        function hold() {
          s.hold = true
          s.to = Qt.point(71, 30)
          var held = s.text
          s.hold = false
          var released = s.text
          s.to = Qt.point(72, 30)
          seen = [held, released, s.text]
        }
      }
      """
    When the method "hold" of "root" is called
    Then the property "seen" of "root" is ["Hello ", "Hello ", "Hello W"]
    And the log has no warnings

  @api-QtQuick.Pdf.PdfSelection.clear @api-QtQuick.Pdf.PdfSelection.textChanged
  Scenario: Clearing a selection forgets its points and its text
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        property int changes: 0
        PdfSelection {
          id: s
          document: d
          page: 0
          from: Qt.point(10, 30)
          to: Qt.point(70, 30)
          onTextChanged: root.changes++
        }
        property var seen: []
        function clear() {
          var before = [s.text, root.changes]
          s.clear()
          seen = before.concat([s.text, s.from.x, s.from.y, s.to.x, s.to.y, root.changes])
        }
      }
      """
    When the method "clear" of "root" is called
    Then the property "seen" of "root" is ["Hello ", 1, "", 0, 0, 0, 0, 2]
    And the log has no warnings

  # Qt 6.11 throws "Unknown method return type: QPdfSelection" from these.
  @qt-diverges
  @api-QtQuick.Pdf.PdfDocument.getSelection @api-QtQuick.Pdf.PdfDocument.getSelectionAtIndex
  @api-QtQuick.Pdf.PdfDocument.getAllText
  Scenario: A script asks a document for the text of a page
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        readonly property var between: d.status === PdfDocument.Ready
          ? d.getSelection(0, Qt.point(10, 30), Qt.point(70, 30)) : null
        readonly property var seen: between
          ? [between.valid, between.text, between.startIndex, between.endIndex,
             between.bounds.length, d.getSelectionAtIndex(0, 6, 5).text,
             d.getAllText(1).text, d.getAllText(9).valid] : []
      }
      """
    Then the property "seen" of "root" is [true, "Hello ", 0, 6, 1, "World", "Page two says hello too", false]
    And the log has no warnings

  @api-QtQuick.Pdf.PdfSelection.copyToClipboard
  Scenario: A selection is copied as text
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        PdfSelection {
          id: s
          document: d
          page: 1
          from: Qt.point(20, 40)
          to: Qt.point(1, 1)
        }
        function copy() { s.selectAll(); s.copyToClipboard() }
      }
      """
    When the method "copy" of "root" is called
    Then the clipboard holds "Page two says hello too"
    And the log has no warnings
