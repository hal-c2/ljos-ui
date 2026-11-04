# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfpageview.html
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfscrollablepageview.html
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfmultipageview.html
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdflinkdelegate.html
# Verified on Qt 6.11.2 (qml6, offscreen). "text.pdf" is the document of
# pdf-text.feature: its first page says "Hello World" and "Second line
# hello" and has a link to https://example.com/ over the first line and one
# to the second page over the second; its second page says "Page two says
# hello too". Qt's multi-page view scrolls to a page by an animation and
# selects all of the page it has scrolled to, so what it selects is not
# checked here.

@qt-parity
Feature: Text, search and links in a PDF viewer
  Each viewer searches its document for its "searchString" through its
  "searchModel" and goes from result to result with "searchForward" and
  "searchBack". Dragging over a page selects its text, "selectAll" selects
  all of the page shown, and "copySelectionToClipboard" copies what is
  selected. Clicking a link to a place in the document goes there. A
  PdfLinkDelegate is the part of a page a link covers.

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

  @api-QtQuick.Pdf.PdfPageView.searchModel @api-QtQuick.Pdf.PdfPageView.searchForward @api-QtQuick.Pdf.PdfPageView.searchBack
  @api-QtQuick.Pdf.PdfScrollablePageView.searchModel @api-QtQuick.Pdf.PdfScrollablePageView.searchForward @api-QtQuick.Pdf.PdfScrollablePageView.searchBack
  @api-QtQuick.Pdf.PdfMultiPageView.searchModel @api-QtQuick.Pdf.PdfMultiPageView.searchForward @api-QtQuick.Pdf.PdfMultiPageView.searchBack
  Scenario Outline: A <view> goes from one place its search found to the next
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      <view> {
        objectName: "v"
        width: 400; height: 300
        document: PdfDocument { source: "text.pdf" }
        searchString: "hello"
        readonly property int found: searchModel.count
        readonly property bool searchesItsDocument: searchModel.document === document
      }
      """
    When 300 milliseconds pass
    Then the property "found" of "v" is 3
    And the property "searchesItsDocument" of "v" is true
    When the method "searchForward" of "v" is called
    And the method "searchForward" of "v" is called
    And 400 milliseconds pass
    Then the property "currentPage" of "v" is 1
    When the method "searchForward" of "v" is called
    And 400 milliseconds pass
    Then the property "currentPage" of "v" is 0
    When the method "searchBack" of "v" is called
    And 400 milliseconds pass
    Then the property "currentPage" of "v" is 1
    And the log has no warnings

    Examples:
      | view                  |
      | PdfPageView           |
      | PdfScrollablePageView |
      | PdfMultiPageView      |

  @api-QtQuick.Pdf.PdfPageView.selectAll @api-QtQuick.Pdf.PdfPageView.selectedText @api-QtQuick.Pdf.PdfPageView.copySelectionToClipboard
  @api-QtQuick.Pdf.PdfScrollablePageView.selectAll @api-QtQuick.Pdf.PdfScrollablePageView.selectedText @api-QtQuick.Pdf.PdfScrollablePageView.copySelectionToClipboard
  Scenario Outline: A <view> selects all of the page it shows
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      <view> {
        objectName: "v"
        width: 400; height: 300
        document: PdfDocument { source: "text.pdf" }
      }
      """
    When 200 milliseconds pass
    Then the property "selectedText" of "v" is ""
    When the method "goToPage" of "v" is called with 1
    And the method "selectAll" of "v" is called
    Then the property "selectedText" of "v" is "Page two says hello too"
    When the method "copySelectionToClipboard" of "v" is called
    Then the clipboard holds "Page two says hello too"
    And the log has no warnings

    Examples:
      | view                  |
      | PdfPageView           |
      | PdfScrollablePageView |

  # The drag begins on a link: the view's drag takes the press from it.
  # The multi-page view sets its page further right than the others, so
  # the drag there ends further right.
  @api-QtQuick.Pdf.PdfPageView.selectedText @api-QtQuick.Pdf.PdfScrollablePageView.selectedText
  @api-QtQuick.Pdf.PdfMultiPageView.selectedText
  Scenario Outline: Dragging over a <view> selects the text between
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      <view> {
        objectName: "v"
        width: 300; height: 200
        document: PdfDocument { source: "text.pdf" }
      }
      """
    When 200 milliseconds pass
    And the user drags from 18, 30 to <end>, 30
    Then the property "selectedText" of "v" is "Hello W"
    And the log has no warnings

    Examples:
      | view                  | end |
      | PdfPageView           | 75  |
      | PdfScrollablePageView | 75  |
      | PdfMultiPageView      | 85  |

  @api-QtQuick.Pdf.PdfPageView.currentPage @api-QtQuick.Pdf.PdfScrollablePageView.currentPage
  @api-QtQuick.Pdf.PdfMultiPageView.currentPage
  Scenario Outline: Clicking a link to a place in the document goes there, in a <view>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      <view> {
        objectName: "v"
        width: 300; height: 200
        document: PdfDocument { source: "text.pdf" }
      }
      """
    When 200 milliseconds pass
    And the user touches 100, 62
    And 300 milliseconds pass
    Then the property "currentPage" of "v" is 1
    And the log has no warnings

    Examples:
      | view                  |
      | PdfPageView           |
      | PdfScrollablePageView |
      | PdfMultiPageView      |

  @api-QtQuick.Pdf.PdfLinkDelegate.tapped @api-QtQuick.Pdf.PdfLinkDelegate.contextMenuRequested
  Scenario: A link delegate says which link was tapped, and which was asked about
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        width: 300; height: 200
        property string tapped
        property int asked: -2
        PdfDocument { id: d; source: "text.pdf" }
        Repeater {
          model: PdfLinkModel { document: d; page: 0 }
          delegate: PdfLinkDelegate {
            x: rectangle.x; y: rectangle.y
            width: rectangle.width; height: rectangle.height
            onTapped: function(link) { root.tapped = link.url }
            onContextMenuRequested: function(link) { root.asked = link.page }
          }
        }
      }
      """
    When 200 milliseconds pass
    And the user touches 50, 30
    Then the property "tapped" of "root" is "https://example.com/"
    When the user clicks the right button at 100, 62
    Then the property "asked" of "root" is 1
    And the log has no warnings

  # Where a link in the document goes is told with the page counted from 1;
  # the place on it is left out here, as Qt measures it with the height of the
  # page the link is on (pdf-links.feature).
  @api-QtQuick.Pdf.PdfLinkDelegate
  Scenario: A link delegate's tip tells where the link goes
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "text.pdf" }
        Repeater {
          id: rows
          model: PdfLinkModel { document: d; page: 0 }
          delegate: PdfLinkDelegate {}
        }
        function tip(row) {
          const tips = rows.itemAt(row).data.filter(c => "delay" in c)
          return tips.map(t => t.text.split(" location")[0] + " after " + t.delay)
        }
        property var seen: []
        function read() { seen = [tip(0), tip(1)] }
      }
      """
    When 200 milliseconds pass
    And the method "read" of "root" is called
    Then the property "seen" of "root" is [["https://example.com/ after 1000"], ["Page 2 after 1000"]]
    And the log has no warnings
