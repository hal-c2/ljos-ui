# The qtdoc gallery

`corpus/qtdoc` is the Qt documentation repository, a git submodule. It is test input only: Ljós
renders its examples and compares them with what `qml6` draws. Nothing in it is read as source,
copied, or edited (CLEAN_ROOM.md). `qml6` is a black box that is run and whose pictures are looked at.

| Path | What |
|---|---|
| `examples.json` | the examples the gallery runs, and how |
| `expected.json` | the committed ratchet |
| `harness/reference.qml` | our own QML that loads an example in `qml6`, waits and saves a picture |
| `harness/overlay.sh` | writes a `qmldir` per CMake-registered module so plain `qml6` finds it |
| `reference/` | the Qt pictures; git-ignored, made on each machine |

## Tasks

```
mise run gallery:reference [id ...]     # qml6 offscreen, one picture per runnable example
mise run gallery [--raise] [--top N] [id ...]
```

`gallery` builds `ljos` and `qml-compare`, renders every example on Ljós (`ljos render`: the R
backend, headless, a virtual clock), scores it against `reference/<id>.png` (pixels and content,
`crates/qml_compare`), writes an HTML report and checks the ratchet. It prints a table, the score
distribution and the ten most common first errors. It exits 1 only when the ratchet is broken.

`GALLERY_DIR` is where the report and the Ljós pictures go (default `target/gallery`); point it at
the scratch drive for long sessions. Every `qml6` and `ljos render` run is under `timeout`.

An example that does not compile or load on Ljós scores 0 with the first error as its reason. That
is data, not a failure: the table is the to-do list for the type library.

## The ratchet

`expected.json` lists examples that render now with a floor per score (the score cut down to a
hundredth). A listed example must keep rendering and keep its floors. An entry without floors (for
example one with no Qt picture) only holds "keeps rendering". `gallery --raise` adds examples that
render and lifts floors to the present scores; it never lowers one. Lowering is done by editing the
file by hand, in a commit that says why.

Without `reference/` the scores are missing, and every floor then reports "no score to hold". Run
`gallery:reference` first. That is why CI runs the gallery nightly where `qml6` is installed, not on
every pull request.

## Why the Qt pictures are not committed

They are Qt output, large (the corpus is 29 PNGs at window size and grows with each example), and
they depend on the Qt version and fonts of the machine that drew them. Each machine takes its own
with `gallery:reference`, so floors compare like with like. The ratchet is the committed result. The
floors in `expected.json` were taken against Qt 6 on the machine that made the first run; another Qt
build may differ by a pixel or two, which the hundredth of rounding absorbs.

## `examples.json`

```
{ "root": "qtdoc",
  "examples": [ { "id", "dir", "entry", "window": {"width","height"}, "kind",
                  "volatile": "time" | "animation" | "network"  (why two pictures differ; told in the report),
                  "qt": { "runnable": bool, "reason": "...", "settle": ms } } ] }
```

`dir` is relative to `corpus/<root>`. `qt.runnable=false` means qml6 cannot draw it headless here
(a missing module, C++ types registered by the app, a `qrc:` entry); the reason says which. Such an
example is still tried on Ljós so its first error is counted, but it has no score.

## Examples that do not start on Ljós, and why

What is left once everything Ljós should have is there. Each needs something an application or a
module outside Qt's product modules provides, so the gap is not closed in Ljós.

| example | what it needs | whose |
|---|---|---|
| android-feature-delivery | `io.qt.fdmaploader`: `MapLoader` is a C++ type the app registers, and `loadedImages` a context property its C++ sets | the app |
| car-configurator | `Qt.labs.assetdownloader` (`AssetDownloader`), a module of Qt's examples written in C++ that fetches the demo's assets; `QtQuick.Studio.Components` (`SvgPathItem`) and `QtQuick.Studio.DesignEffects` (`DesignEffect`), which come with Qt Design Studio and not with Qt; and what it shows is QtQuick3D, which Ljós stands in for | modules outside Qt; Quick3D is a stand-in |

`FrameAnimation`, which car-configurator's wall effects use, is QtQuick's and is in Ljós
(`features/animation/frame-animation.feature`).
