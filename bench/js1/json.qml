import QtQuick
import QtTest
TestCase {
  name: "B"
  property int rounds: 1
  function test_bench() {
    const R = rounds // read once: a property read is not the work measured
    const doc = { items: [] }
    for (let i = 0; i < 1200; ++i)
      doc.items.push({ id: i, name: "name " + i, tags: ["a", "b", "c"], score: i / 7, ok: i % 2 === 0 })
    const text = JSON.stringify(doc)
    const t0 = Date.now()
    let n = 0
    for (let r = 0; r < R; ++r) n += JSON.stringify(JSON.parse(text)).length
    console.log("ms", Date.now() - t0, n, text.length)
  }
}
