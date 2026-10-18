# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#contentWidth-prop
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#completed-signal
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#popup-sizing
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Text.implicitWidth @api-QtQuick.Text.contentWidth
@api-QtQml.Component.completed
Feature: A text is measured by the time it is completed
  A text knows its size when its `Component.onCompleted` runs: what it
  asks for and what its content takes are the sizes it keeps once it is
  shown. So does a control or a popup that asks for the size of the text
  in it.

  Scenario Outline: <what> knows its size when it is completed
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        id: root
        objectName: "root"
        property real early: -1
        property real earlyHeight: -1
        <item> {
          id: subject
          <body>
          Component.onCompleted: {
            root.early = subject.<width>
            root.earlyHeight = subject.<height>
          }
        }
        property bool wide: root.early > 10
        property bool kept: root.early === subject.<width>
        property bool keptHeight: root.earlyHeight > 5 && root.earlyHeight === subject.<height>
      }
      """
    Then the property "wide" of "root" is true
    And the property "kept" of "root" is true
    And the property "keptHeight" of "root" is true

    Examples:
      | what                     | item    | body                                                       | width          | height          |
      | A text                   | Text    | text: "Hello world"                                        | implicitWidth  | implicitHeight  |
      | A text's content         | Text    | text: "Hello world"                                        | contentWidth   | contentHeight   |
      | A rich text              | Text    | textFormat: Text.RichText; text: "<b>Hello</b> world"      | implicitWidth  | implicitHeight  |
      | A label                  | Label   | text: "Hello world"                                        | implicitWidth  | implicitHeight  |
      | A control with a label   | Control | padding: 4; contentItem: Label { text: "Hello world" }     | implicitWidth  | implicitHeight  |
      | A popup with a label     | Popup   | contentItem: Label { text: "Hello world" }                 | implicitWidth  | implicitHeight  |
