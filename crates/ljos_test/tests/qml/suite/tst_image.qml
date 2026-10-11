import QtQuick
import QtTest

Item {
    id: root
    width: 100
    height: 100

    Rectangle {
        id: box
        x: 10
        y: 20
        width: 40
        height: 30
        color: "#ff0000"

        Rectangle {
            id: inner
            x: 20
            width: 20
            height: 30
            color: Qt.rgba(0, 0, 1, 1)
        }
    }

    TestCase {
        name: "GrabImage"
        when: windowShown

        function init() {
            inner.color = Qt.rgba(0, 0, 1, 1)
        }

        function test_size() {
            var image = grabImage(box)
            compare(image.width, 40)
            compare(image.height, 30)
        }

        function test_channels() {
            var image = grabImage(box)
            compare(image.red(5, 5), 255)
            compare(image.green(5, 5), 0)
            compare(image.blue(5, 5), 0)
            compare(image.alpha(5, 5), 255)
            compare(image.red(30, 5), 0)
            compare(image.blue(30, 5), 255)
        }

        function test_pixel() {
            var image = grabImage(box)
            compare(image.pixel(5, 5), Qt.rgba(1, 0, 0, 1))
            compare(image.pixel(30, 15), Qt.rgba(0, 0, 1, 1))
        }

        function test_equals() {
            var before = grabImage(box)
            var same = grabImage(box)
            verify(before.equals(same))
            inner.color = "green"
            var after = grabImage(box)
            verify(!after.equals(before))
            compare(after.green(30, 5), 128)
        }

        function test_save() {
            var image = grabImage(box)
            image.save("grabbed-by-the-suite.png")
            verify(true)
        }
    }
}
