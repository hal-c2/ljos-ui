# Sources:
#   https://doc.qt.io/qt-6/qml-qtcore-standardpaths.html
#   https://doc.qt.io/qt-6/qstandardpaths.html
#   https://specifications.freedesktop.org/basedir-spec/latest/
# Verified on Qt 6.11.2 (qml6, offscreen).

Feature: StandardPaths
  Where the system keeps a user's documents, settings and caches, as file
  URLs. On Linux these follow the XDG base directories.

  Rule: The locations and how they are named

    @qt-parity
    Scenario Outline: StandardPaths.<name> is <value>
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item { objectName: "o"; property var v: StandardPaths.<name> }
        """
      Then the property "v" of "o" is <value>

      Examples:
        | name                  | value |
        | DesktopLocation       | 0     |
        | DocumentsLocation     | 1     |
        | TempLocation          | 7     |
        | HomeLocation          | 8     |
        | ConfigLocation        | 13    |
        | GenericConfigLocation | 16    |
        | AppConfigLocation     | 18    |
        | GenericStateLocation  | 22    |
        | LocateFile            | 0     |
        | LocateDirectory       | 1     |

    @qt-parity @api-QtCore.StandardPaths.displayName
    Scenario Outline: The <location> is called "<name>"
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item { objectName: "o"; property string v: StandardPaths.displayName(StandardPaths.<location>) }
        """
      Then the property "v" of "o" is "<name>"

      Examples:
        | location              | name                  |
        | HomeLocation          | Home                  |
        | DocumentsLocation     | Documents             |
        | TempLocation          | Temporary Directory   |
        | GenericConfigLocation | Shared Configuration  |
        | CacheLocation         | Cache                 |

    @qt-parity @api-QtCore.StandardPaths.findExecutable
    Scenario: A program that is nowhere on the path is not found
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item { objectName: "o"; property string v: StandardPaths.findExecutable("ljos-no-such-program") }
        """
      Then the property "v" of "o" is ""

  # The oracle runs in the home directory of whoever runs it; Ljós's tests
  # give the application a home of their own.
  @ljos-only
  Rule: The user's directories

    @api-QtCore.StandardPaths.writableLocation
    Scenario Outline: The <location> is <url>
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item { objectName: "o"; property string v: StandardPaths.writableLocation(StandardPaths.<location>) }
        """
      Then the property "v" of "o" is "<url>"

      Examples:
        | location              | url                               |
        | HomeLocation          | file:///home/user                 |
        | DocumentsLocation     | file:///home/user/Documents       |
        | GenericConfigLocation | file:///home/user/.config         |
        | GenericDataLocation   | file:///home/user/.local/share    |
        | GenericCacheLocation  | file:///home/user/.cache          |
        | GenericStateLocation  | file:///home/user/.local/state    |

    @api-QtCore.StandardPaths.standardLocations
    Scenario: Shared data is looked for in the user's directory, then the system's
      Given the QML:
        """qml
        import QtQuick
        import QtCore
        Item { objectName: "o"; property var v: StandardPaths.standardLocations(StandardPaths.GenericDataLocation).map(String) }
        """
      Then the property "v" of "o" is ["file:///home/user/.local/share", "file:///usr/local/share", "file:///usr/share"]

    @api-QtCore.StandardPaths.locate
    Scenario: A file is located in the first directory that has it
      Given the user has the text file ".config/tool.conf":
        """
        on=true
        """
      And the QML:
        """qml
        import QtQuick
        import QtCore
        Item {
          objectName: "o"
          property string found: StandardPaths.locate(StandardPaths.GenericConfigLocation, "tool.conf")
          property string missing: StandardPaths.locate(StandardPaths.GenericConfigLocation, "none.conf")
        }
        """
      Then the property "found" of "o" is "file:///home/user/.config/tool.conf"
      And the property "missing" of "o" is ""
