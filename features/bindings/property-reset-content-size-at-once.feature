# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-container.html#contentWidth-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-container.html#contentHeight-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-dialogbuttonbox.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-menubar.html
#
# Qt 6.11.2 accepts undefined for the content size of a DialogButtonBox
# and of a MenuBar and then still reads the size that was given ("false"
# in both scenarios): the size is only worked out again when the buttons
# or menus next change (property-reset-content-size.feature has that for
# the button box). A Pane, and a TabBar, follow their content at once on
# Qt. Here every one of them follows what its content asks for from the
# moment it stops having a size of its own; there is no state in between
# to keep.

@qt-diverges
Feature: A container whose content size is taken back follows its content at once
  Assigning undefined to contentWidth or contentHeight takes the given
  size back. From then on a button box or a menu bar has the size its
  buttons or menus ask for, as one that was never given a size has.

  Scenario: A button box has what its buttons ask for at once
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        DialogButtonBox {
          id: box; contentWidth: 321; contentHeight: 99
          standardButtons: DialogButtonBox.Ok
        }
        DialogButtonBox { id: plain; standardButtons: DialogButtonBox.Ok }
        property bool same: box.contentWidth === plain.contentWidth
          && box.contentHeight === plain.contentHeight
        function back() { box.contentWidth = undefined; box.contentHeight = undefined }
      }
      """
    Then the property "same" of "root" is false
    When the method "back" of "root" is called
    Then the property "same" of "root" is true

  Scenario: A menu bar has what its menus ask for at once
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        objectName: "root"
        MenuBar {
          id: bar; contentWidth: 500; contentHeight: 90
          Menu { title: "File" }
          Menu { title: "Edit" }
        }
        MenuBar {
          id: plain
          Menu { title: "File" }
          Menu { title: "Edit" }
        }
        property bool same: bar.contentWidth === plain.contentWidth
          && bar.contentHeight === plain.contentHeight
        function back() { bar.contentWidth = undefined; bar.contentHeight = undefined }
      }
      """
    Then the property "same" of "root" is false
    When the method "back" of "root" is called
    Then the property "same" of "root" is true
