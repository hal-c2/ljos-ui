// Real Lottie files rather than a generated one: the HAL-C2 avatar's dance
// and genie (features/lottie/avatars, exported from Blender's grease pencil),
// four of each, every one looping for ever. The dance redraws all its
// strokes every frame; the genie grows from nothing, so its cost changes
// through each loop. Eight players at once, as a page of avatars would have.
import QtQuick
import Qt.labs.lottieqt

Rectangle {
    width: 1024
    height: 512
    color: "white"

    Grid {
        columns: 4

        Repeater {
            model: 8

            Item {
                width: 256
                height: 256

                LottieAnimation {
                    source: index % 2 === 0 ? "../features/lottie/avatars/hal-c2-dance.json"
                                            : "../features/lottie/avatars/hal-c2-genie.json"
                    loops: LottieAnimation.Infinite
                    scale: 0.5
                    transformOrigin: Item.TopLeft
                }
            }
        }
    }
}
