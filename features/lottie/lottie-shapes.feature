# Sources:
#   https://doc.qt.io/qt-6/qml-qt-labs-lottieqt-lottieanimation.html
#   https://lottiefiles.github.io/lottie-docs/layers/
#   https://lottiefiles.github.io/lottie-docs/shapes/
#   https://lottiefiles.github.io/lottie-docs/concepts/
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: LottieAnimation draws what the file describes
  A Lottie file is a stack of layers. A shape layer holds rectangles,
  ellipses, stars, polygons and free paths, filled and stroked with colours
  or gradients, grouped and transformed. Layers can be moved, turned, scaled,
  faded and parented; a layer can hold a whole other composition. Values
  change between keyframes along an easing curve.

  Every file here is 100 by 100 and the window behind it is black.

  Scenario: Rectangles, ellipses, polygons and paths are filled
    Given the text file "shapes.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":10,"w":100,"h":100,"layers":[{"ty":4,"ip":0,"op":10,"st":0,"ks":{},"shapes":[
      {"ty":"gr","it":[{"ty":"rc","p":{"a":0,"k":[25,25]},"s":{"a":0,"k":[30,30]},"r":{"a":0,"k":0}},{"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100}}]},
      {"ty":"gr","it":[{"ty":"el","p":{"a":0,"k":[75,25]},"s":{"a":0,"k":[30,30]}},{"ty":"fl","c":{"a":0,"k":[0,1,0,1]},"o":{"a":0,"k":100}}]},
      {"ty":"gr","it":[{"ty":"sr","sy":2,"pt":{"a":0,"k":4},"p":{"a":0,"k":[25,75]},"r":{"a":0,"k":0},"or":{"a":0,"k":20},"os":{"a":0,"k":0}},{"ty":"fl","c":{"a":0,"k":[0,0,1,1]},"o":{"a":0,"k":100}}]},
      {"ty":"gr","it":[{"ty":"sh","ks":{"a":0,"k":{"c":true,"v":[[60,60],[90,60],[75,90]],"i":[[0,0],[0,0],[0,0]],"o":[[0,0],[0,0],[0,0]]}}},{"ty":"fl","c":{"a":0,"k":[1,1,0,1]},"o":{"a":0,"k":100}}]}
      ]}]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { source: "shapes.json"; autoPlay: false }
      }
      """
    Then the pixel at 25, 25 of the window is "#ff0000"
    And the pixel at 75, 25 of the window is "#00ff00"
    And the pixel at 25, 75 of the window is "#0000ff"
    And the pixel at 75, 70 of the window is "#ffff00"
    And the pixel at 62, 12 of the window is "#000000"
    And the pixel at 12, 62 of the window is "#000000"
    And the pixel at 50, 50 of the window is "#000000"

  Scenario: A stroke follows the outline and leaves the inside empty
    Given the text file "stroke.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":10,"w":100,"h":100,"layers":[{"ty":4,"ip":0,"op":10,"st":0,"ks":{},"shapes":[
      {"ty":"rc","p":{"a":0,"k":[50,50]},"s":{"a":0,"k":[60,60]},"r":{"a":0,"k":0}},
      {"ty":"st","c":{"a":0,"k":[1,1,1,1]},"o":{"a":0,"k":100},"w":{"a":0,"k":10},"lc":1,"lj":1,"ml":4}
      ]}]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { source: "stroke.json"; autoPlay: false }
      }
      """
    Then the pixel at 20, 50 of the window is "#ffffff"
    And the pixel at 80, 50 of the window is "#ffffff"
    And the pixel at 50, 20 of the window is "#ffffff"
    And the pixel at 50, 50 of the window is "#000000"

  Scenario: A linear gradient runs from its start to its end
    Given the text file "gradient.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":10,"w":100,"h":100,"layers":[{"ty":4,"ip":0,"op":10,"st":0,"ks":{},"shapes":[
      {"ty":"rc","p":{"a":0,"k":[50,50]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0}},
      {"ty":"gf","t":1,"s":{"a":0,"k":[0,50]},"e":{"a":0,"k":[100,50]},"g":{"p":2,"k":{"a":0,"k":[0,1,0,0,1,0,0,1]}},"o":{"a":0,"k":100}}
      ]}]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { source: "gradient.json"; autoPlay: false }
      }
      """
    Then the pixel at 5, 50 of the window is within 12 of "#f1000e"
    And the pixel at 50, 50 of the window is within 12 of "#7e0081"
    And the pixel at 95, 50 of the window is within 12 of "#0b00f4"

  Scenario: A layer is moved, turned and scaled about its anchor
    Given the text file "transform.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":10,"w":100,"h":100,"layers":[{"ty":4,"ip":0,"op":10,"st":0,
      "ks":{"a":{"a":0,"k":[0,0]},"p":{"a":0,"k":[50,50]},"s":{"a":0,"k":[200,200]},"r":{"a":0,"k":45},"o":{"a":0,"k":100}},"shapes":[
      {"ty":"rc","p":{"a":0,"k":[0,0]},"s":{"a":0,"k":[20,20]},"r":{"a":0,"k":0}},
      {"ty":"fl","c":{"a":0,"k":[1,1,1,1]},"o":{"a":0,"k":100}}
      ]}]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { source: "transform.json"; autoPlay: false }
      }
      """
    Then the pixel at 50, 50 of the window is "#ffffff"
    And the pixel at 50, 25 of the window is "#ffffff"
    And the pixel at 72, 50 of the window is "#ffffff"
    And the pixel at 25, 25 of the window is "#000000"

  Scenario: A layer moves with its parent
    Given the text file "parent.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":10,"w":100,"h":100,"layers":[
      {"ty":4,"ind":2,"parent":1,"ip":0,"op":10,"st":0,"ks":{},"shapes":[
      {"ty":"rc","p":{"a":0,"k":[20,50]},"s":{"a":0,"k":[20,20]},"r":{"a":0,"k":0}},
      {"ty":"fl","c":{"a":0,"k":[1,1,1,1]},"o":{"a":0,"k":100}}]},
      {"ty":3,"ind":1,"ip":0,"op":10,"st":0,"ks":{"p":{"a":0,"k":[30,0]}}}
      ]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { source: "parent.json"; autoPlay: false }
      }
      """
    Then the pixel at 50, 50 of the window is "#ffffff"
    And the pixel at 20, 50 of the window is "#000000"

  Scenario: A layer can hold another composition
    Given the text file "precomp.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":10,"w":100,"h":100,
      "assets":[{"id":"inner","layers":[{"ty":4,"ip":0,"op":10,"st":0,"ks":{},"shapes":[
      {"ty":"rc","p":{"a":0,"k":[10,10]},"s":{"a":0,"k":[20,20]},"r":{"a":0,"k":0}},
      {"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100}}]}]}],
      "layers":[{"ty":0,"refId":"inner","w":100,"h":100,"ip":0,"op":10,"st":0,"ks":{"p":{"a":0,"k":[50,50]}}}]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { source: "precomp.json"; autoPlay: false }
      }
      """
    Then the pixel at 60, 60 of the window is "#ff0000"
    And the pixel at 40, 40 of the window is "#000000"
    And the pixel at 75, 75 of the window is "#000000"

  Scenario: Trimming a path draws only part of it
    Given the text file "trim.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":10,"w":100,"h":100,"layers":[{"ty":4,"ip":0,"op":10,"st":0,"ks":{},"shapes":[
      {"ty":"sh","ks":{"a":0,"k":{"c":false,"v":[[10,50],[90,50]],"i":[[0,0],[0,0]],"o":[[0,0],[0,0]]}}},
      {"ty":"tm","s":{"a":0,"k":0},"e":{"a":0,"k":50},"o":{"a":0,"k":0},"m":1},
      {"ty":"st","c":{"a":0,"k":[1,1,1,1]},"o":{"a":0,"k":100},"w":{"a":0,"k":10},"lc":1,"lj":1}
      ]}]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { source: "trim.json"; autoPlay: false }
      }
      """
    Then the pixel at 30, 50 of the window is "#ffffff"
    And the pixel at 47, 50 of the window is "#ffffff"
    And the pixel at 53, 50 of the window is "#000000"
    And the pixel at 70, 50 of the window is "#000000"

  Scenario: A half transparent layer lets the background through
    Given the text file "opacity.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":10,"w":100,"h":100,"layers":[{"ty":4,"ip":0,"op":10,"st":0,"ks":{"o":{"a":0,"k":50}},"shapes":[
      {"ty":"rc","p":{"a":0,"k":[50,50]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0}},
      {"ty":"fl","c":{"a":0,"k":[1,1,1,1]},"o":{"a":0,"k":100}}
      ]}]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { source: "opacity.json"; autoPlay: false }
      }
      """
    Then the pixel at 50, 50 of the window is within 3 of "#7f7f7f"

  Scenario: A solid layer is a coloured rectangle
    Given the text file "solid.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":10,"w":100,"h":100,"layers":[{"ty":1,"sc":"#ff0000","sw":50,"sh":50,"ip":0,"op":10,"st":0,"ks":{}}]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { source: "solid.json"; autoPlay: false }
      }
      """
    Then the pixel at 25, 25 of the window is "#ff0000"
    And the pixel at 75, 75 of the window is "#000000"

  Scenario: The first layer in the file is on top
    Given the text file "order.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":10,"w":100,"h":100,"layers":[
      {"ty":4,"ip":0,"op":10,"st":0,"ks":{},"shapes":[{"ty":"rc","p":{"a":0,"k":[40,50]},"s":{"a":0,"k":[40,40]},"r":{"a":0,"k":0}},{"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100}}]},
      {"ty":4,"ip":0,"op":10,"st":0,"ks":{},"shapes":[{"ty":"rc","p":{"a":0,"k":[60,50]},"s":{"a":0,"k":[40,40]},"r":{"a":0,"k":0}},{"ty":"fl","c":{"a":0,"k":[0,0,1,1]},"o":{"a":0,"k":100}}]}
      ]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { source: "order.json"; autoPlay: false }
      }
      """
    Then the pixel at 30, 50 of the window is "#ff0000"
    And the pixel at 50, 50 of the window is "#ff0000"
    And the pixel at 70, 50 of the window is "#0000ff"

  Scenario Outline: A value eases between its keyframes
    Given the text file "ease.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":20,"w":100,"h":100,"layers":[{"ty":4,"ip":0,"op":20,"st":0,
      "ks":{"p":{"a":1,"k":[{"t":0,"s":[10,50],"o":{"x":[1],"y":[0]},"i":{"x":[1],"y":[1]}},{"t":20,"s":[90,50]}]}},
      "shapes":[{"ty":"rc","p":{"a":0,"k":[0,0]},"s":{"a":0,"k":[12,20]},"r":{"a":0,"k":0}},
      {"ty":"fl","c":{"a":0,"k":[1,1,1,1]},"o":{"a":0,"k":100}}]}]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { objectName: "anim"; source: "ease.json"; autoPlay: false }
      }
      """
    When the method "gotoAndStop" of "anim" is called with <frame>
    Then the pixel at <white>, 50 of the window is "#ffffff"
    And the pixel at <black>, 50 of the window is "#000000"

    Examples:
      | frame | white | black |
      | 5     | 10    | 20    |
      | 10    | 20    | 30    |
      | 15    | 35    | 25    |

  Scenario Outline: A layer is shown only between its in and out points
    Given the text file "layertime.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":20,"w":100,"h":100,"layers":[
      {"ty":4,"ip":0,"op":10,"st":0,"ks":{},"shapes":[{"ty":"rc","p":{"a":0,"k":[25,50]},"s":{"a":0,"k":[20,20]},"r":{"a":0,"k":0}},{"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100}}]},
      {"ty":4,"ip":10,"op":20,"st":0,"ks":{},"shapes":[{"ty":"rc","p":{"a":0,"k":[75,50]},"s":{"a":0,"k":[20,20]},"r":{"a":0,"k":0}},{"ty":"fl","c":{"a":0,"k":[0,0,1,1]},"o":{"a":0,"k":100}}]}
      ]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { objectName: "anim"; source: "layertime.json"; autoPlay: false }
      }
      """
    When the method "gotoAndStop" of "anim" is called with <frame>
    Then the pixel at 25, 50 of the window is "<left>"
    And the pixel at 75, 50 of the window is "<right>"

    Examples:
      | frame | left    | right   |
      | 5     | #ff0000 | #000000 |
      | 15    | #000000 | #0000ff |

  # Qt does not hold a value whose keyframe says to hold (h 1): it moves the
  # bar anyway. The Lottie format says the value stays until the next key.
  @qt-diverges
  Scenario: A held value stays until the next keyframe
    Given the text file "hold.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":20,"w":100,"h":100,"layers":[{"ty":4,"ip":0,"op":20,"st":0,
      "ks":{"p":{"a":1,"k":[{"t":0,"s":[10,50],"h":1},{"t":20,"s":[90,50]}]}},
      "shapes":[{"ty":"rc","p":{"a":0,"k":[0,0]},"s":{"a":0,"k":[12,20]},"r":{"a":0,"k":0}},
      {"ty":"fl","c":{"a":0,"k":[1,1,1,1]},"o":{"a":0,"k":100}}]}]}
      """
    And the QML:
      """qml
      import QtQuick
      import Qt.labs.lottieqt
      Item {
        width: 100; height: 100
        LottieAnimation { objectName: "anim"; source: "hold.json"; autoPlay: false }
      }
      """
    When the method "gotoAndStop" of "anim" is called with 15
    Then the pixel at 12, 50 of the window is "#ffffff"
    And the pixel at 50, 50 of the window is "#000000"
