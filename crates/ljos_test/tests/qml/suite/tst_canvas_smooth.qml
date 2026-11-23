import QtQuick
import QtTest

// A canvas drawn small and scaled up: smooth: false keeps its pixels hard,
// smooth (the default) blends them.
Item {
    id: root
    width: 128
    height: 256

    component Halves: Canvas {
        width: 16
        height: 16
        scale: 8
        transformOrigin: Item.TopLeft
        onPaint: {
            const ctx = getContext("2d");
            ctx.fillStyle = "#000000";
            ctx.fillRect(0, 0, 8, 16);
            ctx.fillStyle = "#ffffff";
            ctx.fillRect(8, 0, 8, 16);
        }
    }

    Item {
        id: sharp
        width: 128
        height: 128
        Halves { smooth: false }
    }

    Item {
        id: soft
        y: 128
        width: 128
        height: 128
        Halves {}
    }

    TestCase {
        name: "CanvasSmooth"
        when: windowShown

        // Pixels on a row across the black-to-white edge that are neither.
        function greys(item) {
            const image = grabImage(item);
            let n = 0;
            for (let x = 4; x < 124; x++) {
                const red = image.red(x, 40);
                if (red > 8 && red < 247)
                    n++;
            }
            return n;
        }

        function test_hard_edge() {
            wait(200);
            compare(greys(sharp), 0, "grey pixels on a black-to-white edge");
        }

        function test_smooth_edge() {
            wait(200);
            verify(greys(soft) > 0, "a smooth canvas blends its scaled pixels");
        }
    }
}
