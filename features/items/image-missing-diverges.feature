# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-image.html#status-prop
#
# Qt 6.11.2 (qml6, offscreen) warns for every item whose picture cannot be
# opened, so a list whose delegates show one missing icon says it once a
# row. Ljós says it once for the source: an application run without the
# files it downloads should say what is missing, not drown its log in it.

@qt-diverges
Feature: A missing picture is said once
  Every item whose picture is missing still has an error status; the
  log says once of each file that it cannot be opened.

  Scenario: Items that show the same missing file say so once
    Given the QML:
      """qml
      import QtQuick
      Column {
        Repeater {
          model: 3
          Image { objectName: "icon" + index; source: "missing.png" }
        }
        Image { source: "gone.png" }
      }
      """
    Then the property "status" of "icon0" is 3
    And the property "status" of "icon2" is 3
    And the log has 1 warnings matching "QML Image: Cannot open: file:///.*missing\.png$"
    And the log has 1 warnings matching "QML Image: Cannot open: file:///.*gone\.png$"
