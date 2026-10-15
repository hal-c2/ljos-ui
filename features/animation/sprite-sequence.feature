# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-spritesequence.html
#   https://doc.qt.io/qt-6/qml-qtquick-sprite.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: SpriteSequence
  A SpriteSequence plays the frames of one Sprite at a time, cut from an
  image, and goes from one Sprite to another as their `to` maps say.
  strip.png is four 16 by 16 frames in a row: red, green, blue, white;
  sheet.png holds the same four in two rows of two.

  Rule: The sprites and their order

    Background:
      Given the sprite sheet "strip.png"
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string trail: ""
          SpriteSequence {
            objectName: "seq"
            width: 16; height: 16
            interpolate: false
            onCurrentSpriteChanged: trail += currentSprite + " "
            Sprite {
              name: "still"
              source: "strip.png"
              frameCount: 1
            }
            Sprite {
              name: "go"
              source: "strip.png"
              frameCount: 2
              frameDuration: 100
              to: { "still": 1 }
            }
          }
        }
        """

    @api-QtQuick.SpriteSequence.currentSprite
    Scenario: The first sprite plays first, from the first frame
      Then the property "currentSprite" of "seq" is ""
      When 50 milliseconds pass
      Then the property "currentSprite" of "seq" is "still"

    Scenario: A jump before the first frame is lost
      When the method "jumpTo" of "seq" is called with "go"
      And 50 milliseconds pass
      Then the property "currentSprite" of "seq" is "still"

    Scenario: A sprite with nowhere to go plays again
      When 500 milliseconds pass
      Then the property "currentSprite" of "seq" is "still"

    @api-QtQuick.SpriteSequence.jumpTo
    Scenario: jumpTo() goes to the sprite on the next frame
      When 50 milliseconds pass
      And the method "jumpTo" of "seq" is called with "go"
      Then the property "currentSprite" of "seq" is "still"
      When 20 milliseconds pass
      Then the property "currentSprite" of "seq" is "go"

    @api-QtQuick.Sprite.to
    Scenario: When its frames are done a sprite goes where `to` says
      When 50 milliseconds pass
      And the method "jumpTo" of "seq" is called with "go"
      And 300 milliseconds pass
      Then the property "currentSprite" of "seq" is "still"
      And the property "trail" of "root" is "still go still "

    @api-QtQuick.SpriteSequence.running
    Scenario: A sequence that is not running stays where it is
      When 50 milliseconds pass
      And the method "jumpTo" of "seq" is called with "go"
      And the property "running" of "seq" is set to false
      And 300 milliseconds pass
      Then the property "currentSprite" of "seq" is "go"

  Rule: A goal

    @api-QtQuick.SpriteSequence.goalSprite
    Scenario: A goal is reached by the way there, whatever the weights
      Given the sprite sheet "strip.png"
      And the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string trail: ""
          SpriteSequence {
            objectName: "seq"
            width: 16; height: 16
            onCurrentSpriteChanged: trail += currentSprite + " "
            Sprite { name: "a"; source: "strip.png"; frameDuration: 100; to: { "a": 1, "b": 0 } }
            Sprite { name: "b"; source: "strip.png"; frameDuration: 100; to: { "a": 1, "c": 0 } }
            Sprite { name: "c"; source: "strip.png"; frameDuration: 100 }
          }
        }
        """
      When 50 milliseconds pass
      And the property "goalSprite" of "seq" is set to "c"
      And 500 milliseconds pass
      Then the property "currentSprite" of "seq" is "c"
      And the property "trail" of "root" is "a b c "

  Rule: What a new sequence is

    @api-QtQuick.SpriteSequence.interpolate @api-QtQuick.SpriteSequence.running
    Scenario Outline: A new sequence's <property> is <value>
      Given the QML:
        """qml
        import QtQuick
        SpriteSequence { objectName: "seq" }
        """
      Then the property "<property>" of "seq" is <value>

      Examples:
        | property       | value |
        | running        | true  |
        | interpolate    | true  |
        | goalSprite     | ""    |
        | currentSprite  | ""    |
        | width          | 0     |
        | implicitWidth  | 0     |

    Scenario Outline: A new sprite's <property> is <value>
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property Sprite sprite: Sprite {}
        }
        """
      Then the property "sprite.<property>" of "root" is <value>

      Examples:
        | property               | value |
        | frameCount             | 1     |
        | frameDuration          | -2    |
        | frameRate              | -2    |
        | duration               | -1    |
        | frameX                 | 0     |
        | frameY                 | 0     |
        | frameWidth             | 0     |
        | frameHeight            | 0     |
        | frameDurationVariation | 0     |
        | reverse                | false |
        | frameSync              | false |
        | randomStart            | false |
        | name                   | ""    |

  Rule: What is drawn

    Scenario: Nothing is drawn before the first frame
      Given the sprite sheet "strip.png"
      And the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 64; height: 64; color: "black"
          SpriteSequence {
            width: 64; height: 64
            Sprite { source: "strip.png"; frameCount: 4; frameDuration: 100 }
          }
        }
        """
      Then the pixel at 32, 32 of the window is "#000000"

    Scenario Outline: After <ms> ms the frame shown is <color>
      Given the sprite sheet "strip.png"
      And the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 64; height: 64; color: "black"
          SpriteSequence {
            width: 64; height: 64
            interpolate: false
            Sprite { source: "strip.png"; frameCount: 4; frameDuration: 100 }
          }
        }
        """
      When <ms> milliseconds pass
      Then the pixel at 32, 32 of the window is "<color>"

      Examples:
        | ms  | color   |
        | 50  | #ff0000 |
        | 150 | #00ff00 |
        | 250 | #0000ff |
        | 350 | #ffffff |
        | 450 | #ff0000 |

    Scenario Outline: Frames go on to the next row: after <ms> ms, <color>
      Given the sprite sheet "sheet.png"
      And the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 64; height: 64; color: "black"
          SpriteSequence {
            width: 64; height: 64
            interpolate: false
            Sprite {
              source: "sheet.png"; frameCount: 4; frameDuration: 100
              frameWidth: 16; frameHeight: 16
            }
          }
        }
        """
      When <ms> milliseconds pass
      Then the pixel at 32, 32 of the window is "<color>"

      Examples:
        | ms  | color   |
        | 150 | #00ff00 |
        | 250 | #0000ff |

    @api-QtQuick.Sprite.frameX
    Scenario: A sprite starts at its first frame's place
      Given the sprite sheet "strip.png"
      And the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 64; height: 64; color: "black"
          SpriteSequence {
            width: 64; height: 64
            interpolate: false
            Sprite { source: "strip.png"; frameCount: 1; frameX: 32; frameWidth: 16; frameHeight: 16 }
          }
        }
        """
      When 50 milliseconds pass
      Then the pixel at 32, 32 of the window is "#0000ff"

    Scenario Outline: A sequence's size is its own, not its frame's: <property> is <value>
      Given the sprite sheet "strip.png"
      And the QML:
        """qml
        import QtQuick
        Item {
          SpriteSequence {
            objectName: "seq"
            Sprite { source: "strip.png"; frameCount: 4; frameDuration: 100 }
          }
        }
        """
      When 50 milliseconds pass
      Then the property "<property>" of "seq" is <value>

      Examples:
        | property       | value |
        | implicitWidth  | 0     |
        | implicitHeight | 0     |
        | width          | 0     |

    Scenario: A frame with no width set is the image's width shared by the frames
      Given the sprite sheet "strip.png"
      And the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 64; height: 64; color: "black"
          SpriteSequence {
            width: 64; height: 64
            interpolate: false
            Sprite { source: "strip.png"; frameCount: 2; frameDuration: 1000 }
          }
        }
        """
      When 50 milliseconds pass
      Then the pixel at 48, 32 of the window is "#00ff00"
