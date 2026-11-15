# Sources:
#   features/lottie/avatars/README.md (the files, and the GIFs the pictures come from)
#   https://lottiefiles.github.io/lottie-docs/
# The oracle is the preview GIF each file was exported beside, not Qt: the
# frames are what the animation's author drew, and Qt's own player is not
# asked.

@ljos-only
Feature: Real Lottie files play as their author drew them
  Two animations exported from Blender's grease pencil by its Lottie add-on
  (Lottie 5.5.6, 24 frames a second, 512 by 512): a dance that sways left and
  right in a second, and a genie that grows out of nothing into a face in two.
  Each was exported with a preview GIF at 320 by 320, a frame every 40 ms,
  whose frame N is the file's frame N + 1, and some of those frames are kept
  beside the files. Here the animation is drawn at the GIF's size and sent
  to one of those frames, and the window should look like the GIF.

  A GIF has 256 colours at most, and its encoder dithers flat colours and
  moves an edge's blend, so no pixel can be pinned. The comparison blurs both
  pictures by their 3 by 3 neighbourhood and measures each pixel's colour
  difference as CIE76 ΔE: 2.3 is just noticeable, and black to white is 100.
  At most 0.1 % of the pixels (102 of 320 by 320) may differ by more than
  ΔE 15. Ljós's frames come within ΔE 10 of the GIF's everywhere, the
  palette's error; the dance one frame away has 0.2 % of its pixels further
  than ΔE 15, and a lean to the other side 13 %.

  Rule: The genie grows out of nothing into the face

    Scenario Outline: The genie animation plays from nothing to the face
      Given the Lottie file "hal-c2-genie.json"
      And the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Rectangle {
          width: 320; height: 320
          color: "white"
          LottieAnimation {
            id: anim; objectName: "anim"
            source: "hal-c2-genie.json"
            autoPlay: false
            scale: 320 / 512
            transformOrigin: Item.TopLeft
          }
        }
        """
      When the method "gotoAndStop" of "anim" is called with <frame>
      And 100 milliseconds pass
      Then the window's picture looks like the picture "lottie/avatars/<picture>": at most 0.1 % of its pixels differ by more than ΔE 15

      Examples:
        | frame | picture      | what                                   |
        | 1     | genie-01.png | nothing yet                            |
        | 9     | genie-09.png | a wisp rising from the smoke           |
        | 17    | genie-17.png | the screen opening, still without eyes |
        | 25    | genie-25.png | the face, sparkles about it            |
        | 41    | genie-41.png | the face it holds to the end           |

  Rule: The dance sways left and right

    Scenario Outline: The dance animation sways from one side to the other
      Given the Lottie file "hal-c2-dance.json"
      And the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Rectangle {
          width: 320; height: 320
          color: "white"
          LottieAnimation {
            id: anim; objectName: "anim"
            source: "hal-c2-dance.json"
            autoPlay: false
            scale: 320 / 512
            transformOrigin: Item.TopLeft
          }
        }
        """
      When the method "gotoAndStop" of "anim" is called with <frame>
      And 100 milliseconds pass
      Then the window's picture looks like the picture "lottie/avatars/<picture>": at most 0.1 % of its pixels differ by more than ΔE 15

      Examples:
        | frame | picture      | what                 |
        | 1     | dance-01.png | upright              |
        | 7     | dance-07.png | leaning to one side  |
        | 13    | dance-13.png | upright again        |
        | 19    | dance-19.png | leaning to the other |
