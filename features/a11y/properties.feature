# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
#   https://doc.qt.io/qt-6/qtquick-accessibility.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# The documentation gives no defaults for the properties that follow the role;
# what is pinned here is what Qt 6.11.2 answers.

@qt-parity @a11y
Feature: The properties of Accessible
  Accessible has a property for each thing an item can say about itself, and
  each starts out saying nothing.

  Scenario Outline: A property starts out saying nothing
    Given the QML:
      """qml
      import QtQuick
      Rectangle { objectName: "it"; Accessible.name: "It" }
      """
    Then the property "Accessible.<property>" of "it" is <value>

    Examples:
      | property        | value |
      | role            | 0     |
      | description     | ""    |
      | id              | ""    |
      | checkable       | false |
      | checked         | false |
      | checkStateMixed | false |
      | defaultButton   | false |
      | editable        | false |
      | focusable       | false |
      | focused         | false |
      | ignored         | false |
      | multiLine       | false |
      | passwordEdit    | false |
      | pressed         | false |
      | readOnly        | false |
      | searchEdit      | false |
      | selectable      | false |
      | selectableText  | false |
      | selected        | false |
      | labelledBy      | null  |
      | labelFor        | null  |

  Scenario Outline: A property keeps what it is given
    Given the QML:
      """qml
      import QtQuick
      Rectangle { objectName: "it"; Accessible.<property>: <given> }
      """
    Then the property "Accessible.<property>" of "it" is <given>

    Examples:
      | property        | given   |
      | description     | "Saves" |
      | id              | "save"  |
      | checkable       | true    |
      | checked         | true    |
      | checkStateMixed | true    |
      | defaultButton   | true    |
      | editable        | true    |
      | multiLine       | true    |
      | passwordEdit    | true    |
      | pressed         | true    |
      | readOnly        | true    |
      | searchEdit      | true    |
      | selectable      | true    |
      | selectableText  | true    |
      | selected        | true    |

  Rule: Some properties start out following the role

    Scenario Outline: The role decides what an item starts out as
      Given the QML:
        """qml
        import QtQuick
        Rectangle { objectName: "it"; Accessible.role: Accessible.<role> }
        """
      Then the property "Accessible.<property>" of "it" is <value>

      Examples:
        | role         | property  | value |
        | CheckBox     | checkable | true  |
        | RadioButton  | checkable | true  |
        | Switch       | checkable | true  |
        | Button       | checkable | false |
        | MenuItem     | checkable | false |
        | EditableText | editable  | true  |
        | ComboBox     | editable  | false |
        | SpinBox      | editable  | false |
        | StaticText   | readOnly  | true  |
        | EditableText | readOnly  | false |
        | Paragraph    | readOnly  | false |

    Scenario Outline: What the item says wins over what its role would say
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          objectName: "it"
          Accessible.role: Accessible.<role>
          Accessible.<property>: false
        }
        """
      Then the property "Accessible.<property>" of "it" is false

      Examples:
        | role         | property  |
        | CheckBox     | checkable |
        | EditableText | editable  |
        | StaticText   | readOnly  |

    Scenario Outline: A role given later turns the default on
      Given the QML:
        """qml
        import QtQuick
        Rectangle { objectName: "it"; Accessible.name: "It" }
        """
      When the property "Accessible.role" of "it" is set to <number>
      Then the property "Accessible.<property>" of "it" is true

      Examples:
        | number | property  |
        | 44     | checkable |
        | 42     | editable  |
        | 41     | readOnly  |

    Scenario Outline: A default that went on stays on under another role
      Given the QML:
        """qml
        import QtQuick
        Rectangle { objectName: "it"; Accessible.role: Accessible.<role> }
        """
      When the property "Accessible.role" of "it" is set to 43
      Then the property "Accessible.<property>" of "it" is true

      Examples:
        | role         | property  |
        | CheckBox     | checkable |
        | EditableText | editable  |
        | StaticText   | readOnly  |

    Scenario: Being checked does not make an item checkable
      Given the QML:
        """qml
        import QtQuick
        Rectangle { objectName: "it"; Accessible.checked: true }
        """
      Then the property "Accessible.checkable" of "it" is false

  Rule: An item is the label of another

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property bool labelled: field.Accessible.labelledBy === label
          property bool labels: label.Accessible.labelFor === field
          property bool by: other.Accessible.labelledBy === label
          function point() { other.Accessible.labelledBy = label }
          Text { id: label; objectName: "label"; text: "Name"; Accessible.labelFor: field }
          Rectangle { id: field; objectName: "field"; Accessible.role: Accessible.EditableText }
          Rectangle { id: other; objectName: "other"; Accessible.role: Accessible.EditableText }
        }
        """

    Scenario: The item a label is for is labelled by it
      Then the property "labelled" of "root" is true
      And the property "labels" of "root" is true

    Scenario: An item that no label is for is labelled by nothing
      Then the property "Accessible.labelledBy" of "other" is null

    Scenario: An item can say what labels it
      When the method "point" of "root" is called
      Then the property "by" of "root" is true

  Rule: What an item can be asked through Accessible

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root; objectName: "root"
          function plain(html) { return it.Accessible.stripHtml(html) }
          function hide(on) { it.Accessible.setIgnored(on) }
          function say(text) { it.Accessible.announce(text) }
          function alert(text) { it.Accessible.announce(text, Accessible.Assertive) }
          property string stripped: plain("<b>Hi</b> there &amp; you")
          property int polite: Accessible.Polite
          property int assertive: Accessible.Assertive
          Rectangle { id: it; objectName: "it"; Accessible.name: "It" }
        }
        """

    Scenario: stripHtml gives the text without its markup
      Then the property "stripped" of "root" is "Hi there & you"

    Scenario Outline: setIgnored sets the ignored property
      When the method "hide" of "root" is called with <given>
      Then the property "Accessible.ignored" of "it" is <given>

      Examples:
        | given |
        | true  |
        | false |

    Scenario Outline: How urgent an announcement is has a number
      Then the property "<property>" of "root" is <number>

      Examples:
        | property  | number |
        | polite    | 0      |
        | assertive | 1      |

    Scenario Outline: An announcement is made without complaint
      When the method "<method>" of "root" is called with "Saved"
      Then the log has no warnings

      Examples:
        | method |
        | say    |
        | alert  |
