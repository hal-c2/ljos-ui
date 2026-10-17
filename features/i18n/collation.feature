# Sources:
#   https://doc.qt.io/qt-6/qtqml-javascript-functionlist.html
#   https://doc.qt.io/qt-6/qstring.html#localeAwareCompare
#   https://doc.qt.io/qt-6/qcollator.html
#   https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap07.html#tag_07_03_02
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: Text is ordered as the locale orders it
  String.prototype.localeCompare orders two texts by the application's collation: the locale's
  alphabet first, then accents, then case. It is negative, zero or positive as the first comes
  before the second, is the same or comes after it. The collation is the system's LC_COLLATE
  (LC_ALL before it, LANG after it); in the C locale texts are ordered by their code units.

  @qt-parity
  Scenario Outline: A list is sorted by the locale's alphabet
    Given the application starts with the locale environment "<environment>"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string sorted: <list>.sort((x, y) => x.localeCompare(y)).join(" ")
      }
      """
    Then the property "sorted" of "root" is "<sorted>"

    Examples:
      | environment                            | list                               | sorted      |
      | LANG=en_US.UTF-8                       | ["b", "a", "B", "ä", "z"]          | a ä b B z   |
      | LANG=en_US.UTF-8                       | ["ö", "o", "z", "å", "a", "ä"]     | a å ä o ö z |
      | LANG=en_US.UTF-8                       | ["item10", "item2", "Item1"]       | Item1 item10 item2 |
      | LANG=sv_SE.UTF-8                       | ["ö", "o", "z", "å", "a", "ä"]     | a o z å ä ö |
      | LANG=en_US.UTF-8 LC_COLLATE=sv_SE.UTF-8 | ["b", "a", "B", "ä", "z"]         | a b B z ä   |
      | LANG=sv_SE.UTF-8 LC_COLLATE=en_US.UTF-8 | ["ö", "o", "z", "å", "a", "ä"]    | a å ä o ö z |
      | LANG=C                                 | ["b", "a", "B", "ä", "z"]          | B a b z ä   |
      | LANG=C                                 | ["ö", "o", "z", "å", "a", "ä"]     | a o z ä å ö |

  @qt-parity
  Scenario Outline: A comparison is negative, zero or positive
    Given the application starts with the locale environment "<environment>"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int order: "<first>".localeCompare("<second>")
      }
      """
    Then the property "order" of "root" is <order>

    Examples:
      | environment      | first | second | order |
      | LANG=en_US.UTF-8 | a     | b      | -1    |
      | LANG=en_US.UTF-8 | b     | a      | 1     |
      | LANG=en_US.UTF-8 | a     | a      | 0     |
      | LANG=en_US.UTF-8 | a     | A      | -1    |
      | LANG=en_US.UTF-8 | zz    | a      | 1     |
      | LANG=en_US.UTF-8 | e     | é      | -1    |
      | LANG=en_US.UTF-8 |       | a      | -1    |
      | LANG=C           | a     | A      | 32    |
      | LANG=C           | zz    | a      | 25    |
      | LANG=C           | e     | é      | -132  |
      | LANG=C           |       | a      | -1    |

  # Qt orders by the application's collation whatever the second argument
  # is; Ljós takes it, as ECMA-402 does, so a list can be sorted in a
  # locale other than the application's.
  @qt-diverges
  Scenario Outline: A comparison can be made in another locale
    Given the application starts with the locale environment "LANG=en_US.UTF-8"
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property string sorted: ["ö", "o", "z", "å", "a", "ä"].sort((x, y) => x.localeCompare(y, <locale>)).join(" ")
      }
      """
    Then the property "sorted" of "root" is "<sorted>"

    Examples:
      | locale             | sorted      |
      | "sv_SE"            | a o z å ä ö |
      | Qt.locale("sv_SE") | a o z å ä ö |
      | "de"               | a å ä o ö z |
      | Qt.locale()        | a å ä o ö z |
      | undefined          | a å ä o ö z |
      | "C"                | a o z ä å ö |

