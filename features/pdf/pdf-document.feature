# Sources:
#   https://doc.qt.io/qt-6/qtquick-pdf-qmlmodule.html
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfdocument.html
#   https://doc.qt.io/qt-6/qml-qtquick-pdf-pdfpageimage.html
# Verified on Qt 6.11.2 (qmltestrunner, offscreen). The bytes of "info.pdf" are
# a document with a /Info dictionary; those of "locked.pdf" are qpdf's
# "--encrypt secret owner 256" of a document of two pages, 300 by 200 and 100
# by 100. Qt's maxPageHeight reads the larger side of the last page where the
# class page says the tallest page, which is what Ljós does: the document here
# has its tallest page last, where the two agree. Qt asks for a password of a
# literal source before anything can hear; these scenarios bind the source.

@qt-parity
Feature: A PDF document
  A PdfDocument opens the file its "source" names on the spot, and says how
  that went in "status" and "error". What is open stays until something else
  is: an empty "source" only warns. A document that needs a password asks for
  it each time it fails, and setting "password" tries again. This needs the
  "pdf" feature, which the ljos runner has: without it the module is not
  installed, as in a Qt without its PDF plugin.

  Background:
    Given a PDF file "pages.pdf" of pages "300 by 200, 100 by 50, 150 by 400" painted "#ff0000"
    And the file "info.pdf" of hexadecimal bytes:
      """
      255044462d312e340a312030206f626a0a3c3c202f54797065202f436174616c6f67202f5061676573203220302052203e3e0a656e646f626a0a322030206f626a0a3c3c202f54797065202f50616765
      73202f4b696473205b33203020525d202f436f756e742031203e3e0a656e646f626a0a332030206f626a0a3c3c202f54797065202f50616765202f506172656e74203220302052202f4d65646961426f
      78205b30203020313030203130305d202f436f6e74656e7473203420302052203e3e0a656e646f626a0a342030206f626a0a3c3c202f4c656e677468203235203e3e0a73747265616d0a312030203020
      726720302030203130302031303020726520660a656e6473747265616d0a656e646f626a0a352030206f626a0a3c3c202f5469746c65202841205469746c6529202f417574686f722028416e20417574
      686f7229202f5375626a656374202841205375626a65637429202f4b6579776f72647320286b31206b3229202f43726561746f722028437229202f50726f64756365722028507229202f437265617469
      6f6e446174652028443a32303230303130323033303430355a29202f4d6f64446174652028443a32303231303230333034303530365a29203e3e0a656e646f626a0a787265660a3020360a3030303030
      30303030302036353533352066200a30303030303030303039203030303030206e200a30303030303030303538203030303030206e200a30303030303030313135203030303030206e200a3030303030
      3030323032203030303030206e200a30303030303030323737203030303030206e200a747261696c65720a3c3c202f53697a652036202f526f6f74203120302052202f496e666f203520302052203e3e
      0a737461727478726566
      0a3436360a2525454f460a
      """
    And the file "locked.pdf" of hexadecimal bytes:
      """
      255044462d312e370a25bff7a2fe0a312030206f626a0a3c3c202f457874656e73696f6e73203c3c202f41444245203c3c202f4261736556657273696f6e202f312e37202f457874656e73696f6e4c65
      76656c2038203e3e203e3e202f5061676573203220302052202f54797065202f436174616c6f67203e3e0a656e646f626a0a322030206f626a0a3c3c202f436f756e742032202f4b696473205b203320
      302052203420302052205d202f54797065202f5061676573203e3e0a656e646f626a0a332030206f626a0a3c3c202f436f6e74656e7473203520302052202f4d65646961426f78205b20302030203330
      3020323030205d202f506172656e74203220302052202f54797065202f50616765203e3e0a656e646f626a0a342030206f626a0a3c3c202f436f6e74656e7473203620302052202f4d65646961426f78
      205b203020302031303020313030205d202f506172656e74203220302052202f54797065202f50616765203e3e0a656e646f626a0a352030206f626a0a3c3c202f4c656e677468203438202f46696c74
      6572202f466c6174654465636f6465203e3e0a73747265616d0a1d387c169271012166f970be63459193f123443bcf46a758d000f29fbf1379edc503464c460c5ba7c743fa6bd9ce1b77656e64737472
      65616d0a656e646f626a0a362030206f626a0a3c3c202f4c656e677468203438202f46696c746572202f466c6174654465636f6465203e3e0a73747265616d0aaadaff664b6cb13766b8d9bea694a92c
      223f56eb4f1813532d2e90bdc9a333faec826fd4b966fb9725b0c48bd1b6c147656e6473747265616d0a656e646f626a0a372030206f626a0a3c3c202f4346203c3c202f5374644346203c3c202f4175
      74684576656e74202f446f634f70656e202f43464d202f4145535633202f4c656e677468203332203e3e203e3e202f46696c746572202f5374616e64617264202f4c656e67746820323536202f4f203c
      3234393065383630323164323166383636303130386237316364373666623537643438346439373831393730366663663138373263393832646234396537363062316335623236363966316565386538
      646533626234626364383239353134613e202f4f45203c343463623465373839623966356538373136373365613964643830333738626665626632656639643936626361393130376163643062313035
      636663643433663e202f50202d34202f5065726d73203c62376564623735633065316633396164376238643936626138656131333062393e202f522036202f53746d46202f5374644346202f53747246
      202f5374644346202f55203c3738376232633730336434393832336633303534643738356539326535656363323766333064393064313263333362633733396365313431613535663733633564636161
      303237666337393036626634623261306135323039623537663232353e202f5545203c333736633736393436353264346334373738303835623637316132343666613739373136623565363739373835
      353933643233393234653632643162366231363e202f562035203e3e0a656e646f626a0a787265660a3020380a303030303030303030302036353533352066200a303030303030303031352030303030
      30206e200a30303030303030313330203030303030206e200a30303030303030313935203030303030206e200a30303030303030323834203030303030206e200a303030303030303337332030303030
      30206e200a30303030303030343931203030303030206e200a30303030303030363039203030303030206e200a747261696c6572203c3c202f526f6f74203120302052202f53697a652038202f494420
      5b3c33373133373362613762303762336134643032353964333338383038623062303e3c33373133373362613762303762336134643032353964333338383038623062303e5d202f456e637279707420
      3720302052203e3e0a7374617274787265660a313135360a2525454f460a
      """

  @api-QtQuick.Pdf.PdfDocument.status
  Scenario Outline: A document with no source is <expr>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d }
        readonly property var r: <expr>
      }
      """
    Then the property "r" of "root" is <value>
    And the log has no warnings

    Examples:
      | expr                       | value        |
      | d.status                   | 0            |
      | d.status === PdfDocument.Null | true      |
      | PdfDocument.Loading        | 1            |
      | PdfDocument.Ready          | 2            |
      | PdfDocument.Unloading      | 3            |
      | PdfDocument.Error          | 4            |
      | d.pageCount                | 0            |
      | d.error                    | "no error"   |
      | d.title                    | ""           |
      | d.maxPageWidth             | 0            |
      | d.maxPageHeight            | 0            |
      | d.password                 | ""           |
      | d.pagePointSize(0).width   | -1           |
      | isNaN(d.creationDate.getTime()) | true    |
      | isNaN(d.modificationDate.getTime()) | true |

  @api-QtQuick.Pdf.PdfDocument.pageCount @api-QtQuick.Pdf.PdfDocument.pagePointSize
  Scenario Outline: An open document answers <expr>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "pages.pdf" }
        readonly property var r: <expr>
      }
      """
    Then the property "r" of "root" is <value>
    And the log has no warnings

    Examples:
      | expr                          | value      |
      | d.status                      | 2          |
      | d.error                       | "no error" |
      | d.pageCount                   | 3          |
      | d.maxPageWidth                | 300        |
      | d.maxPageHeight               | 400        |
      | d.pagePointSize(0).width      | 300        |
      | d.pagePointSize(0).height     | 200        |
      | d.pagePointSize(2).height     | 400        |
      | d.pagePointSize(3).width      | -1         |
      | d.pagePointSize(-1).height    | -1         |
      | d.title                       | ""         |
      | isNaN(d.creationDate.getTime()) | true     |

  @api-QtQuick.Pdf.PdfDocument.title @api-QtQuick.Pdf.PdfDocument.creationDate
  Scenario Outline: A document says <expr> of itself
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "info.pdf" }
        readonly property var r: <expr>
      }
      """
    Then the property "r" of "root" is <value>

    Examples:
      | expr                             | value                      |
      | d.title                          | "A Title"                  |
      | d.author                         | "An Author"                |
      | d.subject                        | "A Subject"                |
      | d.keywords                       | "k1 k2"                    |
      | d.creator                        | "Cr"                       |
      | d.producer                       | "Pr"                       |
      | d.creationDate.toISOString()     | "2020-01-02T03:04:05.000Z" |
      | d.modificationDate.toISOString() | "2021-02-03T04:05:06.000Z" |

  @api-QtQuick.Pdf.PdfDocument.error
  Scenario Outline: A source of <file> is an error
    Given the file "broken.pdf" of hexadecimal bytes:
      """
      6e6f74206120646f63756d656e740a
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "<file>" }
        property int status: d.status
        property string error: d.error
        property int count: d.pageCount
      }
      """
    Then the property "status" of "root" is 4
    And the property "error" of "root" is "<error>"
    And the property "count" of "root" is 0

    Examples:
      | file       | error               |
      | broken.pdf | invalid file format |
      | nope.pdf   | file not found      |

  @api-QtQuick.Pdf.PdfDocument.source
  Scenario: A new source replaces the document, on the spot
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d }
        property var seen: []
        function open(name) {
          const log = []
          const note = () => log.push(d.status)
          d.statusChanged.connect(note)
          d.source = name
          d.statusChanged.disconnect(note)
          seen = [log.join(","), d.pageCount, d.title, d.error]
        }
      }
      """
    When the method "open" of "root" is called with "pages.pdf"
    Then the property "seen" of "root" is ["1,2", 3, "", "no error"]
    When the method "open" of "root" is called with "pages.pdf"
    Then the property "seen" of "root" is ["", 3, "", "no error"]
    When the method "open" of "root" is called with "info.pdf"
    Then the property "seen" of "root" is ["3,0,1,2", 1, "A Title", "no error"]
    When the method "open" of "root" is called with "nope.pdf"
    Then the property "seen" of "root" is ["3,0,1,4", 0, "", "file not found"]
    When the method "open" of "root" is called with "nope2.pdf"
    Then the property "seen" of "root" is ["1,4", 0, "", "file not found"]
    When the method "open" of "root" is called with "pages.pdf"
    Then the property "seen" of "root" is ["1,2", 3, "", "no error"]
    And the log has no warnings

  @api-QtQuick.Pdf.PdfDocument.source
  Scenario: An empty source leaves what is open with a warning
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        PdfDocument { id: d; source: "pages.pdf" }
        function empty() { d.source = "" }
        property int status: d.status
        property int count: d.pageCount
        property string source: d.source
      }
      """
    When the method "empty" of "root" is called
    Then the property "status" of "root" is 2
    And the property "count" of "root" is 3
    And the property "source" of "root" is ""
    And the log has a warning matching "Cannot open: "

  @api-QtQuick.Pdf.PdfDocument.password @api-QtQuick.Pdf.PdfDocument.passwordRequired
  Scenario: A document asks for its password until it has it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        property int asked: 0
        property string name: "locked.pdf"
        PdfDocument { id: d; source: root.name; onPasswordRequired: root.asked++ }
        function tell(word) { d.password = word }
        property int status: d.status
        property string error: d.error
        property int count: d.pageCount
      }
      """
    Then the property "asked" of "root" is 1
    And the property "status" of "root" is 4
    And the property "error" of "root" is "incorrect password"
    And the property "count" of "root" is 0
    When the method "tell" of "root" is called with "wrong"
    Then the property "asked" of "root" is 2
    And the property "status" of "root" is 4
    When the method "tell" of "root" is called with "wrong"
    Then the property "asked" of "root" is 2
    When the method "tell" of "root" is called with "secret"
    Then the property "asked" of "root" is 2
    And the property "status" of "root" is 2
    And the property "error" of "root" is "no error"
    And the property "count" of "root" is 2

  @api-QtQuick.Pdf.PdfDocument.password
  Scenario: A password declared with the source opens it the first time
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      Item {
        id: root
        objectName: "root"
        property int asked: 0
        PdfDocument {
          id: d; source: "locked.pdf"; password: "secret"
          onPasswordRequired: root.asked++
        }
        property int status: d.status
      }
      """
    Then the property "status" of "root" is 2
    And the property "asked" of "root" is 0

  @api-QtQuick.Pdf.PdfPageImage.document
  Scenario: A page image shows a page of its document
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      PdfPageImage {
        id: i
        objectName: "image"
        currentFrame: 2
        document: PdfDocument { source: "pages.pdf" }
      }
      """
    When 200 milliseconds pass
    Then the property "status" of "image" is 1
    And the property "frameCount" of "image" is 3
    And the property "width" of "image" is 150
    And the property "height" of "image" is 400

  @api-QtQuick.Pdf.PdfPageImage.document
  Scenario: A page image of a locked document shows once it is open
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      PdfPageImage {
        objectName: "image"
        document: PdfDocument { source: "locked.pdf"; password: "secret" }
      }
      """
    When 200 milliseconds pass
    Then the property "status" of "image" is 1
    And the property "frameCount" of "image" is 2
    And the property "width" of "image" is 300

  @api-QtQuick.Pdf.PdfPageImage.document
  Scenario: A page image follows its document to another file
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Pdf
      PdfPageImage {
        objectName: "image"
        document: PdfDocument { id: d; source: "pages.pdf" }
        function other() { d.source = "info.pdf" }
      }
      """
    When 200 milliseconds pass
    And the method "other" of "image" is called
    And 200 milliseconds pass
    Then the property "status" of "image" is 1
    And the property "frameCount" of "image" is 1
    And the property "width" of "image" is 100
