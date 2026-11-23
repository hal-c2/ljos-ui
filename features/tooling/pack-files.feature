# Sources:
#   docs/ARCHITECTURE.md principle 5 (the program links no parser)
#   docs/site/rust/overview.md (a pack built ahead of time, `include_qml!`)
#   https://doc.qt.io/qt-6/qt-add-qml-module.html (RESOURCES, the module's
#   files kept inside the program at `qrc:/qt/qml/<uri as a path>/`)
#   https://doc.qt.io/qt-6/resources.html (the Qt resource system)
#   https://github.com/hal-c2/ljos-ui/issues/702
# Qt's build keeps the files a module lists inside the program and names
# them by `qrc:` addresses; a file a QML file names relative to itself is
# found beside it there. Ljós's pack keeps the files beside the QML
# itself, so that the program needs no list of them and no code to put
# them where the QML looks.

@ljos-only @headless
Feature: A pack carries the files its QML loads
  A program shipped with its QML compiled into a pack has no directory
  of files beside it. The pack carries the files under its QML's
  directories that QML loads (pictures, fonts, models, sounds, films,
  shaders and the data a model reads), and the program that runs the pack finds each
  where the QML names it: relative to the file that names it, or at its
  module's `qrc:` address. Which kinds of files go in is the build's
  choice, besides those a module's build keeps (`RESOURCES`); a file whose
  link leads out of the project stays out. The pack also says which of the QML files it was given did not
  compile, those the compile could not read among them, and why.

  Rule: The files beside the QML load in the packed application

    Scenario: A font and a picture named relative to the QML load
      Given the font file "LjosSymbolsTest-Regular.woff2"
      And a 4 by 4 image file "pics/dot.png" filled with "#ff0000"
      And the file "Main.qml":
        """qml
        import QtQuick
        Item {
            FontLoader { id: font; source: "LjosSymbolsTest-Regular.woff2" }
            Image { id: picture; source: "pics/dot.png" }
            Timer {
                interval: 10; running: true; repeat: true
                onTriggered: {
                    if (font.status === FontLoader.Loading
                            || picture.status === Image.Loading)
                        return
                    console.log("font", font.status, "picture",
                                picture.status, picture.sourceSize.width)
                    Qt.quit()
                }
            }
        }
        """
      When the packed application runs
      Then the packed application's errors mention "debug: font 1 picture 1 4"
      And the packed application's errors do not mention "Cannot load"

    Scenario: A picture of a module loads at the module's address
      Given a 4 by 4 image file "Shapes/icons/dot.png" filled with "#00ff00"
      And the file "Shapes/qmldir":
        """
        module Shapes
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        Item {
            Image {
                id: picture
                source: "qrc:/qt/qml/Shapes/icons/dot.png"
                onStatusChanged: {
                    console.log("picture", status)
                    if (status !== Image.Loading)
                        Qt.quit()
                }
            }
        }
        """
      When the packed application runs
      Then the packed application's errors mention "debug: picture 1"

  Rule: Which files the pack carries is the build's choice

    Background:
      Given the font file "LjosSymbolsTest-Regular.woff2"
      And a 4 by 4 image file "pics/dot.png" filled with "#ff0000"
      And the text file "notes.txt":
        """
        Not for the program.
        """
      And the text file "fx.frag":
        """
        #version 440
        void main() {}
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        Item {}
        """

    Scenario: By default the pack carries the kinds of files QML loads
      When the project is packed
      Then the pack carries the files:
        | file                          |
        | LjosSymbolsTest-Regular.woff2 |
        | fx.frag                       |
        | pics/dot.png                  |

    Scenario: A build that names the kinds gets those alone
      When the project is packed carrying only "png" files
      Then the pack carries the files:
        | file         |
        | pics/dot.png |

    Scenario: A build can carry no files besides the QML
      When the project is packed carrying no other files
      Then the pack carries no files besides its QML

    Scenario: A file a module's build keeps is carried whatever kinds the build takes
      Given the text file "Shapes/data/table.csv":
        """
        side,count
        """
      And the file "Shapes/CMakeLists.txt":
        """
        qt_add_qml_module(shapes URI Shapes RESOURCES data/table.csv)
        """
      When the project is packed carrying no other files
      Then the pack carries the files:
        | file                  |
        | Shapes/data/table.csv |
      And the pack names its files at the addresses:
        | address                           | file                  |
        | qrc:/qt/qml/Shapes/data/table.csv | Shapes/data/table.csv |

    Scenario: A file of the project that links to one outside it is not carried
      Given the file "private.json" outside the project:
        """
        {"secret": true}
        """
      And the project's "data.json" is a link to "private.json" outside the project
      When the project is packed
      Then the pack carries the files:
        | file                          |
        | LjosSymbolsTest-Regular.woff2 |
        | fx.frag                       |
        | pics/dot.png                  |

  Rule: A module's address names the files of its own directory

    Scenario: A module's file is at its address, and a file at its path elsewhere is not
      Given a 4 by 4 image file "Shapes/icons/dot.png" filled with "#00ff00"
      And the file "Shapes/qmldir":
        """
        module Shapes
        Ring 1.0 Ring.qml
        """
      And the file "Shapes/Ring.qml":
        """qml
        import QtQuick
        Item {}
        """
      And a 4 by 4 image file "old/icons/dot.png" filled with "#0000ff"
      And the file "old/qmldir":
        """
        module Shapes
        Ring 1.0 Ring.qml
        """
      And the file "old/Ring.qml":
        """qml
        import QtQuick
        Item {}
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        Item {}
        """
      When the project is packed
      Then the pack names its files at the addresses:
        | address                          | file                 |
        | qrc:/qt/qml/Shapes/icons/dot.png | Shapes/icons/dot.png |

    Scenario: A file kept by the build of a module no import sees is carried without its address
      Given the file "Shapes/qmldir":
        """
        module Shapes
        Ring 1.0 Ring.qml
        """
      And the file "Shapes/Ring.qml":
        """qml
        import QtQuick
        Item {}
        """
      And the file "Shapes/CMakeLists.txt":
        """
        qt_add_qml_module(shapes URI Shapes RESOURCES data/new.csv)
        """
      And the file "Shapes/data/new.csv":
        """
        new
        """
      And the file "old/CMakeLists.txt":
        """
        qt_add_qml_module(old_shapes URI Shapes RESOURCES data/old.csv)
        """
      And the file "old/data/old.csv":
        """
        old
        """
      And the file "Main.qml":
        """qml
        import QtQuick
        Item {}
        """
      When the project is packed carrying no other files
      Then the pack carries the files:
        | file                |
        | Shapes/data/new.csv |
        | old/data/old.csv    |
      And the pack names its files at the addresses:
        | address                         | file                |
        | qrc:/qt/qml/Shapes/data/new.csv | Shapes/data/new.csv |

  Rule: The pack says which QML files did not compile

    Background:
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {}
        """

    Scenario: A file that does not compile is in the pack with why
      Given the file "Broken.qml":
        """qml
        import QtQuick
        Item {
        """
      When the project is packed with the files that do not compile
      Then the pack says these files did not compile:
        | file   | why                               |
        | Broken | 3:1: Expected `}` but found `EOF` |

    Scenario: A file the compile cannot read is in the pack with why
      Given the file "Garbled.qml" of hexadecimal bytes:
        """
        49 74 65 6d 20 7b 20 ff fe 20 7d
        """
      When the project is packed
      Then the pack says these files did not compile:
        | file    | why       |
        | Garbled | not UTF-8 |

    Scenario: A file the compile does not follow a link to is in the pack with why
      Given the file "Lost.qml" outside the project:
        """qml
        import QtQuick
        Item {}
        """
      And the project's "Lost.qml" is a link to "Lost.qml" outside the project
      When the project is packed
      Then the pack says these files did not compile:
        | file | why    |
        | Lost | a link |
