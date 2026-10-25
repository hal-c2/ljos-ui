import QtQuick
import QtTest
TestCase {
  name: "B"
  property int rounds: 1
  function test_bench() {
    const R = rounds // read once: a property read is not the work measured
    const t0 = Date.now()
    let n = 0
    for (let r = 0; r < R; ++r) {
      let s = ""
      for (let i = 0; i < 1000; ++i) s += "item " + i + ", "
      const parts = []
      for (let i = 0; i < 1000; ++i) parts.push(`row ${i}: ${i * 3}`)
      n += s.length + parts.join("\n").length + s.indexOf("item 999")
    }
    console.log("ms", Date.now() - t0, n)
  }
}
