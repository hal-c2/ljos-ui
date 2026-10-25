import QtQuick
import QtTest
TestCase {
  name: "B"
  property int rounds: 1
  function test_bench() {
    const R = rounds // read once: a property read is not the work measured
    const t0 = Date.now()
    let s = 0
    for (let i = 0; i < R; ++i)
      s += Math.sin(i) * Math.cos(i / 3) + Math.sqrt(i) + Math.floor(i / 7) + Math.max(i % 13, 5)
    console.log("ms", Date.now() - t0, s)
  }
}
