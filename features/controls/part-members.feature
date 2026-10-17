# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#grouped-properties
#   https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html#popup-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#background-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Members written through a control's parts
  A property that holds an object is a group: `popup.width: 77` gives `width` to the popup the
  combo box holds, and `background.opacity: 0.5` to the button's background. The parts the style
  gives are there to be written to as the control is declared, as a part the application gives
  is.

  Rule: A member written through a part is the part's

    Scenario Outline: <written> on a <type>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          width: 200; height: 200
          <type> { objectName: "control"; <written> }
        }
        """
      Then the property "<read>" of "control" is <value>

      Examples:
        | type      | written                                           | read                  | value |
        | ComboBox  | model: ["a"]; popup.width: 77                     | popup.width           | 77    |
        | ComboBox  | model: ["a"]; popup.topMargin: 3                  | popup.topMargin       | 3     |
        | ComboBox  | model: ["a"]; popup.scale: 0.5                    | popup.scale           | 0.5   |
        | ComboBox  | model: ["a"]; popup.transformOrigin: Item.TopLeft | popup.transformOrigin | 0     |
        | ComboBox  | model: ["a"]; popup.modal: true                   | popup.modal           | true  |
        | ComboBox  | model: ["a"]; indicator.visible: false            | indicator.visible     | false |
        | Button    | text: "b"; background.opacity: 0.5                | background.opacity    | 0.5   |
        | Button    | text: "b"; contentItem.opacity: 0.25              | contentItem.opacity   | 0.25  |
        | CheckBox  | text: "c"; indicator.opacity: 0.5                 | indicator.opacity     | 0.5   |
        | TextField | background.opacity: 0.5                           | background.opacity    | 0.5   |
        | Popup     | background.opacity: 0.5                           | background.opacity    | 0.5   |

    Scenario: A binding through a part follows what it reads
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          id: root
          objectName: "root"
          width: 200; height: 200
          property int wide: 50
          ComboBox { objectName: "control"; model: ["a"]; popup.width: root.wide * 2 }
        }
        """
      Then the property "popup.width" of "control" is 100
      When the property "wide" of "root" is set to 60
      Then the property "popup.width" of "control" is 120

    Scenario: A handler through a part handles the part's signal
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls
        Item {
          objectName: "root"
          width: 200; height: 200
          property int heard: 0
          Button { objectName: "control"; text: "b"; background.onOpacityChanged: heard++ }
        }
        """
      When the property "background.opacity" of "control" is set to 0.5
      Then the property "heard" of "root" is 1
