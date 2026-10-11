# Sources:
#   https://doc.qt.io/qt-6/qtqml-typesystem-enumerations.html
#   https://doc.qt.io/qt-6/qml-qtquick-text.html
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#fillMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-flow.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-scrollbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-checkbox.html#checkState-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html#display-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Every enumeration property takes its keys by name
  What Item.transformOrigin does with a script's value, every property
  that holds an enumeration does: the name of one of its keys is that
  key, and any other text is refused.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      import QtQuick.Layouts
      Item {
        Text { objectName: "Text" }
        Image { objectName: "Image" }
        BorderImage { objectName: "BorderImage" }
        Row { objectName: "Row" }
        Flow { objectName: "Flow" }
        Grid { objectName: "Grid" }
        GridLayout { objectName: "GridLayout" }
        RowLayout { objectName: "RowLayout" }
        TextInput { objectName: "TextInput" }
        Flickable { objectName: "Flickable" }
        TapHandler { objectName: "TapHandler" }
        ScrollBar { objectName: "ScrollBar" }
        CheckBox { objectName: "CheckBox" }
        Button { objectName: "Button" }
        ToolBar { objectName: "ToolBar" }
        RotationAnimation { objectName: "RotationAnimation" }
        SmoothedAnimation { objectName: "SmoothedAnimation" }
      }
      """

  Scenario Outline: "<key>" is a key of <type>.<property>
    When the property "<property>" of "<type>" is set to "<key>"
    Then the property "<property>" of "<type>" is <number>

    Examples:
      | type              | property                | key                 | number |
      | Text              | horizontalAlignment     | AlignHCenter        | 4      |
      | Text              | verticalAlignment       | AlignBottom         | 64     |
      | Text              | wrapMode                | WrapAnywhere        | 3      |
      | Text              | elide                   | ElideMiddle         | 2      |
      | Text              | lineHeightMode          | FixedHeight         | 1      |
      | Image             | fillMode                | PreserveAspectCrop  | 2      |
      | Image             | horizontalAlignment     | AlignRight          | 2      |
      | Image             | verticalAlignment       | AlignTop            | 32     |
      | BorderImage       | horizontalTileMode      | Round               | 2      |
      | BorderImage       | verticalTileMode        | Repeat              | 1      |
      | Row               | layoutDirection         | RightToLeft         | 1      |
      | Flow              | flow                    | TopToBottom         | 1      |
      | Flow              | layoutDirection         | RightToLeft         | 1      |
      | Grid              | flow                    | TopToBottom         | 1      |
      | Grid              | horizontalItemAlignment | AlignHCenter        | 4      |
      | Grid              | verticalItemAlignment   | AlignBottom         | 64     |
      | GridLayout        | flow                    | TopToBottom         | 1      |
      | RowLayout         | layoutDirection         | RightToLeft         | 1      |
      | TextInput         | echoMode                | Password            | 2      |
      | TextInput         | horizontalAlignment     | AlignRight          | 2      |
      | Flickable         | boundsBehavior          | StopAtBounds        | 0      |
      | Flickable         | flickableDirection      | VerticalFlick       | 2      |
      | TapHandler        | gesturePolicy           | ReleaseWithinBounds | 2      |
      | ScrollBar         | orientation             | Horizontal          | 1      |
      | ScrollBar         | policy                  | AlwaysOn            | 2      |
      | ScrollBar         | snapMode                | SnapAlways          | 1      |
      | CheckBox          | checkState              | PartiallyChecked    | 1      |
      | Button            | display                 | TextUnderIcon       | 3      |
      | Button            | focusPolicy             | ClickFocus          | 2      |
      | ToolBar           | position                | Footer              | 1      |
      | RotationAnimation | direction               | Counterclockwise    | 3      |
      | RotationAnimation | easing.type             | InOutQuad           | 3      |
      | SmoothedAnimation | reversingMode           | Sync                | 2      |

  Scenario Outline: "<text>" is no key of <type>.<property>
    Then setting the property "<property>" of "<type>" to "<text>" is rejected

    Examples:
      | type              | property            | text          |
      | Text              | horizontalAlignment | AlignTop      |
      | Text              | elide               | Nowhere       |
      | Image             | fillMode            | stretch       |
      | Row               | layoutDirection     | Horizontal    |
      | Flow              | flow                | RightToLeft   |
      | GridLayout        | flow                | 1             |
      | ScrollBar         | orientation         | LeftToRight   |
      | ScrollBar         | policy              | Always        |
      | CheckBox          | checkState          | checked       |
      | Button            | display             | IconUnderText |
      | TapHandler        | gesturePolicy       | AlignLeft     |
      | RotationAnimation | easing.type         | Quad          |
