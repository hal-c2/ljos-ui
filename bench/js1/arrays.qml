import QtQuick
import QtTest
TestCase {
  name: "B"
  property int rounds: 1
  function test_bench() {
    const R = rounds // read once: a property read is not the work measured
    const a = []
    for (let i = 0; i < 10000; ++i) a.push((i * 7919) % 10007)
    const t0 = Date.now()
    let s = 0
    for (let r = 0; r < R; ++r) {
      const b = a.map(x => x * 2).filter(x => x % 3 !== 0)
      b.sort((x, y) => x - y)
      s += b.reduce((p, x) => p + x, 0) + b[r % b.length]
    }
    console.log("ms", Date.now() - t0, s)
  }
}
