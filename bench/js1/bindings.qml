import QtQuick
import QtTest
TestCase {
  name: "B"
  property int rounds: 1
  Item { id: root; width: 400; height: 300 }
  Component {
    id: c
    Item {
      width: parent.width / 2 + 4
      height: width * 2 - 1
      property string label: "w=" + width + " h=" + height
      property real k: width > 100 ? width : 100
      property bool big: height > 300 && width < 1000
      property int n: Math.max(width, height)
      property string kind: big ? "big" : "small"
      property real sum: k + n + (big ? 1 : 0)
    }
  }
  function test_bench() {
    const R = rounds // read once: a property read is not the work measured
    const t0 = Date.now()
    let s = 0
    for (let i = 0; i < R; ++i) { const o = c.createObject(root); s += o.sum; o.destroy() }
    console.log("ms", Date.now() - t0, s)
  }
}
