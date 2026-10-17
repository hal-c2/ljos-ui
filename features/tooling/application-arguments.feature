# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-application.html (arguments)
# Verified on Qt 6.11.2: `qml6 args.qml /x/y` gives ["qml6", "args.qml",
# "/x/y"], the program as it was started, the file as it was given and the
# rest. `ljos run FILE -- ARGS` gives the same, without `run`, its options
# and the `--`, as qml6 leaves out its own options; that is checked in
# crates/ljos_cli/src/run.rs, since it opens a window. `ljos render` gives
# the program and the entry file below the directory it was given.

@ljos-only @headless
Feature: The command line an application was started with
  `Application.arguments` holds the command line the application was
  started with, so that an application can take a file or a folder from
  it, as a Qt application does.

  Scenario: A rendered application is told its entry file
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
        Component.onCompleted: {
          console.warn("arguments " + Application.arguments.length)
          console.warn("folder " + "file:" + Application.arguments.slice(-1)[0])
        }
      }
      """
    When the author runs "ljos render . --out shot.png --settle 0"
    Then the command's output mentions "arguments 2"
    And the command's output mentions "folder file:./Main.qml"
    And the command's output does not mention "undefined"
