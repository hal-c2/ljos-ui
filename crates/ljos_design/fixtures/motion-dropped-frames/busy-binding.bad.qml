// expect: motion-dropped-frames at 10
// audit: reduced-motion=false
import QtQuick

Rectangle {
    width: 300; height: 200
    Rectangle {
        width: 40; height: 40
        color: "steelblue"
        NumberAnimation on x {
            from: 0; to: 200
            duration: 1000
            loops: Animation.Infinite
        }
        // Every step of x runs 30 ms of script.
        onXChanged: {
            const until = Date.now() + 30
            while (Date.now() < until) {}
        }
    }
}
