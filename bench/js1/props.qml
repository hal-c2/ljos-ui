import QtQuick
import QtTest
TestCase {
  name: "B"
  property int rounds: 1
  QtObject { id: a; property int x: 0; property int y: 1 }
  Item { id: b }
  function test_bench() {
    const R = rounds // read once: a property read is not the work measured
    const t0 = Date.now()
    for (let i = 0; i < R; ++i) {
      a.x = a.y + 1
      a.y = a.x % 1000
      b.x = a.y + b.width
    }
    console.log("ms", Date.now() - t0, a.x, b.x)
  }
}
