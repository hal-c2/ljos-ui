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

C++ of an app's own: a mock the app ships is found (`backend_mock/Backend`). Otherwise
`corpus/standins/<id>/` holds QML for it and `gallery:run` aliases each module there; what the app's
`main` does to its root is in `ljos_gallery_backends::launcher_calls`.

| App | Starts | Picture opened | What's wrong | Cause |
|---|---|---|---|---|
| dice | yes | yes: settings panel over an empty grey room, no table, no dice | nothing can be thrown; the table is not in view | `QtQuick3D.Physics` is not built, its types stand in as empty Items |
| osmbuildings | yes | yes: blue, the token line, nothing else | no map, no buildings, no controls | `Fog` missing, `Quaternion.fromEulerAngles` missing, `OSMManager`, `OSMGeometry` and `CustomTextureData` are C++, a map needs the network and a token |
| toycustomizer | yes | yes: "Tap to Start", no toy | no toy, nothing loads | assets are downloaded at build time; `QtQuick3D.Effects` and `QtQuick.Timeline` stand in as empty Items |
| robotarm | yes (mock found) | yes, on the GPU: controls and circles at the right joints, no arm | the arm is not drawn; `+Material/` files are not chosen | `Model.source` takes only built-in meshes, `.mesh` files are left out without a word; file selectors not built |
| car-configurator | yes (stand-in) | yes: "Built with Qt", "Initializing..." | stays at the splash | not yet known; material and environment properties missing (`clearcoatAmount`, `indexOfRefraction`, `specularAmount`, `blendMode`, `receivesReflections`, `fog`, ...), `MorphTarget`, `QtQuick.Timeline`, `Particles3D`, `Effects` |
| fx-material-showroom | yes | yes: "Downloading Assets..." for ever | never reaches its screen | stand-in for `StudioApplication` and assets are there; the launcher's `downloadComplete()` not yet confirmed; `.mesh` as for robotarm |

Not in the example list:

- **documentviewer** is a Qt Widgets app. Its only QML is `plugins/Q3DViewer/Viewer.qml`, which needs
  `RuntimeLoader` from `QtQuick3D.AssetUtils` and a C++ host that sets the file. It cannot run as an
  app here.
- **xr_physicsbase_teleportation** has an `XrView` as its root and needs a headset. XR stays out.
