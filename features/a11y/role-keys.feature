# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#role-prop
#   https://doc.qt.io/qt-6/qaccessible.html#Role-enum
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @a11y
Feature: The keys of Accessible.role
  Every role the documentation lists is a key of Accessible, with Qt's number.

  Scenario Outline: A role key has Qt's number
    Given the QML:
      """qml
      import QtQuick
      Rectangle { objectName: "it"; Accessible.role: Accessible.<key> }
      """
    Then the property "Accessible.role" of "it" is <number>

    Examples:
      | key                  | number |
      | NoRole               | 0      |
      | TitleBar             | 1      |
      | MenuBar              | 2      |
      | ScrollBar            | 3      |
      | Grip                 | 4      |
      | Sound                | 5      |
      | Cursor               | 6      |
      | Caret                | 7      |
      | AlertMessage         | 8      |
      | Window               | 9      |
      | Client               | 10     |
      | PopupMenu            | 11     |
      | MenuItem             | 12     |
      | ToolTip              | 13     |
      | Application          | 14     |
      | Document             | 15     |
      | Pane                 | 16     |
      | Chart                | 17     |
      | Dialog               | 18     |
      | Border               | 19     |
      | Grouping             | 20     |
      | Separator            | 21     |
      | ToolBar              | 22     |
      | StatusBar            | 23     |
      | Table                | 24     |
      | ColumnHeader         | 25     |
      | RowHeader            | 26     |
      | Column               | 27     |
      | Row                  | 28     |
      | Cell                 | 29     |
      | Link                 | 30     |
      | HelpBalloon          | 31     |
      | Assistant            | 32     |
      | List                 | 33     |
      | ListItem             | 34     |
      | Tree                 | 35     |
      | TreeItem             | 36     |
      | PageTab              | 37     |
      | PropertyPage         | 38     |
      | Indicator            | 39     |
      | Graphic              | 40     |
      | StaticText           | 41     |
      | EditableText         | 42     |
      | Button               | 43     |
      | CheckBox             | 44     |
      | RadioButton          | 45     |
      | ComboBox             | 46     |
      | ProgressBar          | 48     |
      | Dial                 | 49     |
      | HotkeyField          | 50     |
      | Slider               | 51     |
      | SpinBox              | 52     |
      | Canvas               | 53     |
      | Animation            | 54     |
      | Equation             | 55     |
      | ButtonDropDown       | 56     |
      | ButtonMenu           | 57     |
      | ButtonDropGrid       | 58     |
      | Whitespace           | 59     |
      | PageTabList          | 60     |
      | Clock                | 61     |
      | Splitter             | 62     |
      | LayeredPane          | 128    |
      | Terminal             | 129    |
      | Desktop              | 130    |
      | Paragraph            | 131    |
      | WebDocument          | 132    |
      | Section              | 133    |
      | Notification         | 134    |
      | Switch               | 135    |
      | ColorChooser         | 1028   |
      | Footer               | 1038   |
      | Form                 | 1040   |
      | Heading              | 1044   |
      | Note                 | 1051   |
      | ComplementaryContent | 1068   |
      | BlockQuote           | 1073   |
      | UserRole             | 65535  |
      | PushButton           | 43     |
