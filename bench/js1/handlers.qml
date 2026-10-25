import QtQuick
import QtTest
TestCase {
  name: "B"
  property int rounds: 1
  property int total: 0
  signal ping(int i)
  onPing: (i) => total += step(i)
  function step(i) { return (i * 2 + 1) % 7 }
  function test_bench() {
    const R = rounds // read once: a property read is not the work measured
    const t0 = Date.now()
    for (let i = 0; i < R; ++i) ping(i)
    console.log("ms", Date.now() - t0, total)
  }
}
