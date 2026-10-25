// A mid-sized Lottie animation played for ever: 40 shape layers with
// eased keyframes, gradients, dashed strokes and trims, and a precomp
// used four times (bench/lottie/gen.py writes it). The cost of drawing
// a Lottie file, frame after frame.
import QtQuick
import Qt.labs.lottieqt

Rectangle {
    width: 512
    height: 512
    color: "black"

    LottieAnimation {
        source: "lottie/mid.json"
        loops: LottieAnimation.Infinite
    }
}
