# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-tabbar.html
#   https://doc.qt.io/qt-6/qml-qtquick-controls-swipeview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A tab bar and a swipe view that follow each other
  The documentation of both binds each one's current index to the
  other's. Moving either moves the other, and the two bindings are no
  binding loop: the change stops at the one that already has the value.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Column {
        width: 300
        TabBar {
          id: bar; objectName: "bar"; width: parent.width; currentIndex: view.currentIndex
          TabButton { text: "Home" }
          TabButton { text: "Mail" }
          TabButton { text: "Other" }
        }
        SwipeView {
          id: view; objectName: "view"; width: parent.width; height: 150; currentIndex: bar.currentIndex
          Item { }
          Item { }
          Item { }
        }
      }
      """

  @api-QtQuick.Controls.TabBar.currentIndex @api-QtQuick.Controls.SwipeView.currentIndex
  Scenario: Both start at the first page without a loop
    Then the property "currentIndex" of "bar" is 0
    And the property "currentIndex" of "view" is 0
    And the log has no warnings

  @api-QtQuick.Controls.Container.setCurrentIndex
  Scenario: The bar moves the view
    When the method "setCurrentIndex" of "bar" is called with 1
    Then the property "currentIndex" of "view" is 1
    And the property "currentIndex" of "bar" is 1
    And the log has no warnings

  @api-QtQuick.Controls.Container.decrementCurrentIndex
  Scenario: The view moves the bar
    When the method "setCurrentIndex" of "view" is called with 2
    And the method "decrementCurrentIndex" of "view" is called
    Then the property "currentIndex" of "bar" is 1
    And the property "currentIndex" of "view" is 1
    And the log has no warnings

  Scenario: An assignment ends only the binding it replaces
    When the property "currentIndex" of "bar" is set to 2
    Then the property "currentIndex" of "view" is 2
    When the property "currentIndex" of "view" is set to 1
    Then the property "currentIndex" of "bar" is 2
    And the property "currentIndex" of "view" is 1
