import QtQuick
import QtTest
TestCase {
  name: "B"
  property int rounds: 1
  ListModel { id: m }
  function test_bench() {
    const R = rounds // read once: a property read is not the work measured
    for (let i = 0; i < 1000; ++i) m.append({ name: "n" + i, value: i })
    const t0 = Date.now()
    let s = 0
    for (let r = 0; r < R; ++r) {
      for (let i = 0; i < m.count; ++i) s += m.get(i).value
      m.setProperty(r % m.count, "value", r)
    }
    console.log("ms", Date.now() - t0, s)
  }
}
