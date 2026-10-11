# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html#role-prop
#   https://doc.qt.io/qt-6/qaccessible.html#Role-enum
#   docs/research/05-accessibility-and-localisation.md (section 2.4)
#   docs/ARCHITECTURE.md (section 14)
# The Qt oracle cannot read the accessibility tree through QML alone, so these
# scenarios run on Ljós only. The word in the second column is what the
# platform's accessibility interface is told the item is.

@ljos-only @a11y
Feature: What each role is to an assistive technology
  Every role of Accessible.role has a counterpart that a screen reader knows.

  Scenario Outline: An item with a role is told to a screen reader as its counterpart
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        Rectangle {
          width: 80; height: 30
          Accessible.role: Accessible.<role>
          Accessible.name: "It"
        }
      }
      """
    Then the accessibility tree has a "<told as>" named "It"

    Examples:
      | role                 | told as           |
      | TitleBar             | titlebar          |
      | MenuBar              | menubar           |
      | ScrollBar            | scrollbar         |
      | Grip                 | splitter          |
      | Sound                | audio             |
      | AlertMessage         | alert             |
      | Window               | window            |
      | Client               | genericcontainer  |
      | PopupMenu            | menu              |
      | MenuItem             | menuitem          |
      | ToolTip              | tooltip           |
      | Application          | application       |
      | Document             | document          |
      | Pane                 | pane              |
      | Chart                | image             |
      | Dialog               | dialog            |
      | Border               | genericcontainer  |
      | Grouping             | group             |
      | Separator            | splitter          |
      | ToolBar              | toolbar           |
      | StatusBar            | status            |
      | Table                | table             |
      | ColumnHeader         | columnheader      |
      | RowHeader            | rowheader         |
      | Column               | genericcontainer  |
      | Row                  | row               |
      | Cell                 | cell              |
      | Link                 | link              |
      | HelpBalloon          | tooltip           |
      | Assistant            | genericcontainer  |
      | List                 | list              |
      | ListItem             | listitem          |
      | Tree                 | tree              |
      | TreeItem             | treeitem          |
      | PageTab              | tab               |
      | PropertyPage         | tabpanel          |
      | Indicator            | meter             |
      | Graphic              | image             |
      | StaticText           | label             |
      | EditableText         | textinput         |
      | Button               | button            |
      | CheckBox             | checkbox          |
      | RadioButton          | radiobutton       |
      | ComboBox             | combobox          |
      | ProgressBar          | progressindicator |
      | Dial                 | slider            |
      | HotkeyField          | textinput         |
      | Slider               | slider            |
      | SpinBox              | spinbutton        |
      | Canvas               | canvas            |
      | Animation            | image             |
      | Equation             | math              |
      | ButtonDropDown       | button            |
      | ButtonMenu           | button            |
      | ButtonDropGrid       | button            |
      | PageTabList          | tablist           |
      | Clock                | time              |
      | Splitter             | splitter          |
      | LayeredPane          | genericcontainer  |
      | Terminal             | terminal          |
      | Desktop              | pane              |
      | Paragraph            | paragraph         |
      | WebDocument          | document          |
      | Section              | section           |
      | Notification         | status            |
      | Switch               | switch            |
      | ColorChooser         | colorwell         |
      | Footer               | footer            |
      | Form                 | form              |
      | Heading              | heading           |
      | Note                 | note              |
      | ComplementaryContent | complementary     |
      | BlockQuote           | blockquote        |
      | UserRole             | unknown           |
      | PushButton           | button            |

  # Ours: a role that says nothing leaves the item what an item with a name
  # and no role is, a named group.
  Scenario Outline: A role that says nothing leaves a named item a group
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        Rectangle {
          width: 80; height: 30
          Accessible.role: Accessible.<role>
          Accessible.name: "It"
        }
      }
      """
    Then the accessibility tree has a "group" named "It"

    Examples:
      | role       |
      | NoRole     |
      | Cursor     |
      | Caret      |
      | Whitespace |

  Scenario: A role that says nothing and no name leaves the item out
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        Rectangle { width: 80; height: 30; Accessible.role: Accessible.Whitespace }
        Rectangle {
          y: 40; width: 80; height: 30
          Accessible.role: Accessible.Button; Accessible.name: "Save"
        }
      }
      """
    Then the accessibility tree has a "button" named "Save"
    And the accessibility tree has no "group" named ""

  Scenario: A dial is a slider that is called a dial
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 200; height: 100
        Rectangle {
          width: 80; height: 30
          Accessible.role: Accessible.Dial; Accessible.name: "Volume"
        }
      }
      """
    Then the "slider" named "Volume" is called a "dial"
