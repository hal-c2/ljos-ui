# The corpus's 3D apps on Ljós

The acceptance test of the Quick3D track. An app is done when its picture was opened and has its 3D
content, `mise run gallery:drive <id>` clicked through it, and its log has no error nobody can
explain. A passing scenario is not evidence. `qt6-quick3d` is not installed here, so there is no Qt
picture to compare with: pictures are judged by eye, and against the old web runtime's where it took
one.

Run `mise run gallery:assets <id>` first for the apps whose build downloads assets. Take pictures
with `LJOS_CUA_GPU=/dev/dri/renderD128` and a `LJOS_CUA_HOME` of their own: a sandbox without a GPU
has no adapter on this machine (`radeon` and `nvidia` drivers only, no `lavapipe`), and every 3D view
stays empty without a word. The first pictures were taken that way, so read "empty" in them with care.

An app's C++ takes one of these routes, none written for one app in the toolkit, and a type on none
of them is a compile error whose note says the project's CMake lists native sources:

1. A mock the app ships is found (`backend_mock/Backend`).
2. `corpus/standins/<id>/` holds QML for the module, and `gallery:run` aliases each module there.
   With stand-ins present it sets `QML_XHR_ALLOW_FILE_READ`, so they can read the app's files.
3. `Geometry` and `TextureData` are writable from QML, so a stand-in can replace a C++ subclass of them.
4. A Rust crate in `examples/` registers the type, listed in `examples/gallery_backends`; what the
   app's `main` does to its root is in `ljos_gallery_backends::launcher_calls`.
5. `corpus/assets.json` and `mise run gallery:assets <id>` fetch what the build would download.

Not every app in this answer is 3D: stocqt below is here because its `StockEngine` is the same case
as robotarm's `Backend` and car-configurator's `AssetDownloader`.

| App | Starts | Picture opened | What's wrong | Cause |
|---|---|---|---|---|
| dice | yes | yes: wooden table, rug and sky; a throw lands 11 dice in the table, settings panel works | nothing known | real physics through `QtQuick3D.Physics` on rapier3d |
| osmbuildings | yes, release profile only | yes: map tiles and buildings from the network, fog to the horizon | a white highlight at the lower left at the start and roofs whiter than Qt's doc picture; dragging and zooming not driven; a debug build hits the JS heap limit; about 5.5 GB; while tiles arrive one turn of `onMapsDataReady` took 20.8 s, so a click timed out and two more were not seen | stand-ins over `Geometry` and `TextureData` in QML that build every mesh in script; the highlight's cause is specular at the shader's `FRESNEL_POWER` 1.0, judged against the doc picture, not against Qt running |
| toycustomizer | yes | yes: the bear turns on its start page; the drive went Start, Choose, Back, Exit and each changed the page's items, its picture is of the start page it ended on | `ToyAnimations.qml:353` logs `name of undefined` twice, then recovers | the app's binding runs before `node.currentElement` is set; not checked against Qt |
| robotarm | yes (mock found) | yes: the arm follows the sliders, claw and dark mode work, `+Material/` files chosen | nothing known | |
| car-configurator | yes (stand-in, downloaded assets) | yes: car on the desert floor, a click opens its door | wall effects are absent; `ShapeAffector3D`'s `targetVelocity` has no effect; turn-budget warnings in debug; the drive reached only the door, the settings panel was not clicked | `WallEffect*` ships only compiled `.qsb` shaders and Ljós has no qsb reader (ARCHITECTURE.md, on purpose) |
| fx-material-showroom | yes (stand-in) | yes: all 9 materials change the dragon; rain, smoke, heatwave, sparks and flames show; models None, Qt ball and dragon work | snow, dust and clouds faint or not visible in the shot taken, not settled; `floor.frag` names `paramsTexture`, which it never declares, so the studio floor's shader fails; a pale blue strip under the dragon is unexplained; the rabbit model was not clicked | black bands are the app's own `Scale` letterbox |
| stocqt | yes (stand-in) | yes: list of stocks with logos, prices and changes | the chart is empty; the stand-in's data is the app's sample files, not live prices | `QtGraphs` is not part of Ljós and stands in as empty Items |

| App | Native type | Route | State |
|---|---|---|---|
| robotarm | `Backend` | mock the app ships | works |
| car-configurator | `AssetDownloader` | `corpus/standins/car-configurator`, `gallery:assets` | works |
| car-configurator | `ShapeAffector3D` | `examples/carconfigurator` | works, except as in the table |
| fx-material-showroom | `StudioApplication` | `corpus/standins/fx-material-showroom`, `launcher_calls` | works |
| stocqt | `StockEngine`, `TimeFormatter` (QML singleton of module `StocQt`) | `corpus/standins/stocqt`, reads `data/*.json` by `XMLHttpRequest` | works |
| osmbuildings | `OSMGeometry`, `OSMManager`, tile textures | `corpus/standins/osmbuildings`, over `Geometry` and `TextureData` | works, as in the table |

Not in the example list:

- **documentviewer** is a Qt Widgets app. Its only QML is `plugins/Q3DViewer/Viewer.qml`, which needs
  `RuntimeLoader` from `QtQuick3D.AssetUtils` and a C++ host that sets the file. It cannot run as an
  app here.
- **xr_physicsbase_teleportation** has an `XrView` as its root and needs a headset. XR stays out.
