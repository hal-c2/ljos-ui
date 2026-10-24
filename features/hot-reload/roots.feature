# Sources:
#   docs/ARCHITECTURE.md (section 12.1)
#   features/window/replace-root.feature (the window a new root takes)
# Hot reload is a Ljós extension: Qt has nothing to compare it with.

@ljos-only
Feature: Hot reload over every directory of an application's import path
  An application that finds its QML in several directories, a user's own
  first, is watched in all of them. A file in an earlier directory hides
  one of the same name in a later one, and a module of one directory is
  imported by the files of another. The application names the files its
  window may show, in order: at the start and at every reload the first of
  them that is there and compiles is shown. A switch to another file
  starts it afresh in the same window; a reload that keeps the file keeps
  its state.

  Background:
    Given the file "app/Main.qml":
      """qml
      import QtQuick
      import Lib
      Item {
        objectName: "main"
        Badge {
          objectName: "box"
          property int taps: 0
          function tap() { taps += 1 }
        }
      }
      """
    And the file "lib/Lib/qmldir":
      """
      module Lib
      Badge 1.0 Badge.qml
      """
    And the file "lib/Lib/Badge.qml":
      """qml
      import QtQuick
      Rectangle { width: 10; height: 10 }
      """
    And the QML:
      """qml
      import QtQuick
      Item { objectName: "root" }
      """
    When the application watches the roots "user, lib, app" for the entries "override, Main"

  Scenario: A module of another directory is imported, and its saves reload
    When the method "tap" of "box" is called
    And the file "lib/Lib/Badge.qml" is saved on disk as:
      """qml
      import QtQuick
      Rectangle { width: 30; height: 10 }
      """
    Then the property "width" of "box" is 30
    And the property "taps" of "box" is 1
    And there is no reload error

  Scenario: A file new to a directory is there for the next reload
    When the file "app/Extra.qml" is saved on disk as:
      """qml
      import QtQuick
      Item { objectName: "extra" }
      """
    And the file "app/Main.qml" is saved on disk as:
      """qml
      import QtQuick
      Item {
        objectName: "main"
        Extra { }
      }
      """
    Then the object "extra" exists
    And there is no reload error

  Scenario: A file named earlier that appears takes the window, and gives it back when it goes
    When the file "user/override.qml" is saved on disk as:
      """qml
      import QtQuick
      import Lib
      Item {
        objectName: "override"
        Badge { objectName: "mine" }
      }
      """
    Then the object "override" exists
    And the object "main" does not exist
    And the property "width" of "mine" is 10
    When the file "user/override.qml" is removed from disk
    Then the object "main" exists
    And the object "override" does not exist
    And there is no reload error

  Scenario: A file named earlier that no longer compiles gives the window to the next
    When the file "user/override.qml" is saved on disk as:
      """qml
      import QtQuick
      Item { objectName: "override" }
      """
    And the file "user/override.qml" is saved on disk as:
      """qml
      import QtQuick
      Item {
      """
    Then the object "main" exists
    And the reload error is in "override.qml" at line 3
    When the file "user/override.qml" is saved on disk as:
      """qml
      import QtQuick
      Item { objectName: "mended" }
      """
    Then the object "mended" exists
    And the object "main" does not exist
    And there is no reload error

  Scenario: A file of an earlier directory hides the one of the same name in a later one
    When the file "user/Main.qml" is saved on disk as:
      """qml
      import QtQuick
      Item { objectName: "users" }
      """
    Then the object "users" exists
    And the object "main" does not exist
