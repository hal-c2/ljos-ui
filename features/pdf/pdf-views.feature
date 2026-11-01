# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfmultipageview.html
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfpageview.html
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfscrollablepageview.html
# Verified on Qt 6.11.2 (qmltestrunner, offscreen). goToPage takes any page: the
# multi-page view's currentPage reads what it was given, past the end too, and
# past either end of the single-page view comes round to the other. Qt's
# scrollable view fits its own size whatever scaleToWidth is given, where the
# class page says the given viewport, so these give it its own. Not checked here: where the views have
# scrolled to, which qml6 reaches by an animation.

@qt-parity
Feature: Viewers of a PDF document
  PdfMultiPageView shows every page one below the other and scrolls, PdfPageView
  is as large as the one page it shows, and PdfScrollablePageView scrolls around
  one page. Each goes to a page with "goToPage", remembers where it has been for
  "back" and "forward", and is scaled by "renderScale" or to fit.

  Background:
    Given a PDF file "pages.pdf" of pages "300 by 200, 150 by 400, 100 by 100" painted "#ff0000"

  @api-QtQuick.Pdf.PdfMultiPageView.goToPage @api-QtQuick.Pdf.PdfMultiPageView.currentPage
  Scenario: A multi-page view goes from page to page
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      PdfMultiPageView {
        objectName: "v"
        width: 400; height: 300
        document: PdfDocument { source: "pages.pdf" }
      }
      """
    When 300 milliseconds pass
    Then the property "currentPage" of "v" is 0
    And the property "renderScale" of "v" is 1
    And the property "pageRotation" of "v" is 0
    And the property "backEnabled" of "v" is false
    And the property "forwardEnabled" of "v" is false
    And the property "currentPageRenderingStatus" of "v" is 1
    And the property "searchString" of "v" is ""
    And the property "selectedText" of "v" is ""
    When the method "goToPage" of "v" is called with 1
    Then the property "currentPage" of "v" is 1
    And the property "backEnabled" of "v" is true
    And the property "forwardEnabled" of "v" is false
    When 500 milliseconds pass
    Then the property "currentPage" of "v" is 1
    When the method "back" of "v" is called
    And 500 milliseconds pass
    Then the property "currentPage" of "v" is 0
    And the property "backEnabled" of "v" is false
    And the property "forwardEnabled" of "v" is true
    When the method "forward" of "v" is called
    And 500 milliseconds pass
    Then the property "currentPage" of "v" is 1
    And the log has no warnings

  @api-QtQuick.Pdf.PdfMultiPageView.goToPage
  Scenario: A multi-page view reads back any page it was sent to
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      PdfMultiPageView {
        objectName: "v"
        width: 400; height: 300
        document: PdfDocument { source: "pages.pdf" }
      }
      """
    When 200 milliseconds pass
    And the method "goToPage" of "v" is called with 99
    And 300 milliseconds pass
    Then the property "currentPage" of "v" is 99
    When the method "goToPage" of "v" is called with -1
    And 300 milliseconds pass
    Then the property "currentPage" of "v" is -1

  @api-QtQuick.Pdf.PdfMultiPageView.goToLocation
  Scenario: Going to a location sets the zoom, and going back sets it again
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      PdfMultiPageView {
        objectName: "v"
        width: 400; height: 300
        document: PdfDocument { source: "pages.pdf" }
        function there() { goToLocation(2, Qt.point(10, 20), 2) }
      }
      """
    When 200 milliseconds pass
    And the method "there" of "v" is called
    And 400 milliseconds pass
    Then the property "currentPage" of "v" is 2
    And the property "renderScale" of "v" is 2
    When the method "back" of "v" is called
    And 300 milliseconds pass
    Then the property "currentPage" of "v" is 0
    And the property "renderScale" of "v" is 1

  @api-QtQuick.Pdf.PdfMultiPageView.scaleToWidth @api-QtQuick.Pdf.PdfMultiPageView.scaleToPage
  Scenario Outline: A multi-page view is scaled by <call>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      PdfMultiPageView {
        objectName: "v"
        width: 400; height: 300
        pageRotation: <rotation>
        document: PdfDocument { source: "pages.pdf" }
        function act() { <call> }
      }
      """
    When 200 milliseconds pass
    And the method "act" of "v" is called
    Then the property "renderScale" of "v" is <scale>

    Examples:
      | rotation | call                              | scale |
      | 0        | scaleToWidth(150, 100)            | 0.5   |
      | 90       | scaleToWidth(100, 100)            | 0.5   |
      | 0        | scaleToPage(150, 200)             | 0.5   |
      | 0        | scaleToPage(600, 100)             | 0.5   |
      | 0        | scaleToWidth(150, 100); resetScale() | 1  |

  @api-QtQuick.Pdf.PdfMultiPageView.currentPage
  Scenario: What a multi-page view says of itself is not written from outside
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      PdfMultiPageView {
        objectName: "v"
        width: 400; height: 300
        document: PdfDocument { source: "pages.pdf" }
        property bool refused: false
        function write() { try { currentPage = 2 } catch (e) { refused = true } }
      }
      """
    When the method "write" of "v" is called
    Then the property "refused" of "v" is true
    And the property "currentPage" of "v" is 0

  @api-QtQuick.Pdf.PdfMultiPageView.document
  Scenario: A multi-page view requires its document
    Given the file "Viewer.qml":
      """qml
      import QtQuick.Pdf
      PdfMultiPageView { }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string said
        Component.onCompleted: {
          const c = Qt.createComponent("Viewer.qml")
          said = c.createObject(this) ? "made" : "refused"
        }
      }
      """
    Then the property "said" of "root" is "refused"

  @api-QtQuick.Pdf.PdfPageView.goToPage
  Scenario: A page view is as large as the page it shows
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      PdfPageView {
        objectName: "v"
        document: PdfDocument { source: "pages.pdf" }
      }
      """
    When 200 milliseconds pass
    Then the property "width" of "v" is 300
    And the property "height" of "v" is 200
    And the property "currentPage" of "v" is 0
    And the property "status" of "v" is 1
    And the property "color" of "v" is "#ffffff"
    And the property "zoomEnabled" of "v" is true
    When the method "goToPage" of "v" is called with 1
    And 200 milliseconds pass
    Then the property "width" of "v" is 150
    And the property "height" of "v" is 400
    And the property "currentPage" of "v" is 1
    And the property "backEnabled" of "v" is true
    When the method "goToPage" of "v" is called with -1
    And 200 milliseconds pass
    Then the property "currentPage" of "v" is 2
    And the property "width" of "v" is 100
    When the method "goToPage" of "v" is called with 5
    And 200 milliseconds pass
    Then the property "currentPage" of "v" is 0

  @api-QtQuick.Pdf.PdfPageView.scaleToWidth
  Scenario: A page view fits what it is asked to
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      PdfPageView {
        objectName: "v"
        document: PdfDocument { source: "pages.pdf" }
      }
      """
    When 200 milliseconds pass
    And the method "scaleToWidth" of "v" is called with 100, 100
    And 50 milliseconds pass
    Then the property "width" of "v" is 100
    And the property "height" of "v" is 67
    When the method "resetScale" of "v" is called
    And 50 milliseconds pass
    Then the property "width" of "v" is 300
    When the method "scaleToPage" of "v" is called with 600, 100
    And 50 milliseconds pass
    Then the property "width" of "v" is 150
    And the property "height" of "v" is 100

  @api-QtQuick.Pdf.PdfScrollablePageView.goToPage
  Scenario: A scrollable page view scrolls around the page it shows
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      PdfScrollablePageView {
        objectName: "v"
        width: 200; height: 150
        document: PdfDocument { source: "pages.pdf" }
      }
      """
    When 200 milliseconds pass
    Then the property "contentWidth" of "v" is 300
    And the property "contentHeight" of "v" is 200
    And the property "currentPage" of "v" is 0
    And the property "status" of "v" is 1
    When the method "goToPage" of "v" is called with 1
    And 200 milliseconds pass
    Then the property "contentWidth" of "v" is 150
    And the property "contentHeight" of "v" is 400
    And the property "currentPage" of "v" is 1
    And the property "backEnabled" of "v" is true

  @api-QtQuick.Pdf.PdfScrollablePageView.renderScale @api-QtQuick.Pdf.PdfScrollablePageView.pageRotation
  Scenario: A scrollable page view is scaled and turned
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      PdfScrollablePageView {
        objectName: "v"
        width: 200; height: 150
        document: PdfDocument { source: "pages.pdf" }
        function zoom() { goToPage(1); renderScale = 2 }
      }
      """
    When 200 milliseconds pass
    And the method "zoom" of "v" is called
    And 100 milliseconds pass
    Then the property "contentWidth" of "v" is 300
    And the property "contentHeight" of "v" is 800
    When the property "pageRotation" of "v" is set to 90
    And 100 milliseconds pass
    Then the property "contentWidth" of "v" is 800
    And the property "contentHeight" of "v" is 300
    When the method "scaleToWidth" of "v" is called with 200, 150
    Then the property "renderScale" of "v" is 0.5

  @api-QtQuick.Pdf.PdfStyle.selectionColor
  Scenario: A style names the colours of what is marked on a page
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        objectName: "root"
        PdfStyle { id: s }
        readonly property var r: ["" + s.selectionColor, "" + s.pageSearchResultsColor,
                                  "" + s.currentSearchResultStrokeColor,
                                  s.currentSearchResultStrokeWidth]
      }
      """
    Then the property "r" of "root" is ["#80308cc6", "#80b0c4de", "#00ffff", 2]
