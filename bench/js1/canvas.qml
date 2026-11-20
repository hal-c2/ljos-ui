import QtQuick
import QtTest
TestCase {
  name: "B"
  property int rounds: 1
  width: 64
  height: 64
  when: windowShown
  Canvas { id: canvas; width: 64; height: 64 }
  function test_bench() {
    tryVerify(() => canvas.available)
    const ctx = canvas.getContext("2d")
    const R = rounds // read once: a property read is not the work measured
    const colors = ["#e0a070", "#30406a", "#ffffff", "#202020", "#c03030", "#70a050"]
    const t0 = Date.now()
    let s = 0
    // A round is a sprite sheet: 2000 one-pixel rectangles on whole
    // pixels, each in its own colour, then a read that has them drawn.
    for (let i = 0; i < R; ++i) {
      for (let p = 0; p < 2000; ++p) {
        ctx.fillStyle = colors[(p + i) % 6]
        ctx.fillRect(p % 50, (p / 50) | 0, 1, 1)
      }
      s += ctx.getImageData(i % 50, 0, 1, 1).data[3]
    }
    console.log("ms", Date.now() - t0, s)
  }
}
