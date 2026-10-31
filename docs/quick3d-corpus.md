# The corpus's 3D apps on Ljós

The acceptance test of the Quick3D track. An app is done when its picture was opened and has its 3D
content, `mise run gallery:drive <id>` clicked through it, and its log has no error nobody can
explain. A passing scenario is not evidence. `qt6-quick3d` is not installed here, so there is no Qt
picture to compare with: pictures are judged by eye, and against the old web runtime's where it took
one.

Run `mise run gallery:assets <id>` first for the apps whose build downloads assets.

| App | Starts | Picture opened | What's wrong | Cause |
|---|---|---|---|---|
| dice | yes | yes: settings panel over an empty grey room, no table, no dice | nothing can be thrown; the table is not in view | `QtQuick3D.Physics` is not built, its types stand in as empty Items |
| osmbuildings | yes | yes: blue, the token line, nothing else | no map, no buildings, no controls | `Fog` missing, `Quaternion.fromEulerAngles` missing, `OSMManager`, `OSMGeometry` and `CustomTextureData` are C++, a map needs the network and a token |
| toycustomizer | yes | yes: "Tap to Start", no toy | no toy, nothing loads | assets are downloaded at build time; `QtQuick3D.Effects` and `QtQuick.Timeline` stand in as empty Items |
| robotarm | no | no picture | `Backend` is not a type | the C++-only `Backend/` directory hides the mock the app ships in `backend_mock/Backend` |
| car-configurator | no | no picture | `AssetDownloader` is not a type | a C++ type; 483 MB of assets; `QtQuick.Studio.*`, `QtQuick.Timeline`, `QtQuick3D.Particles3D`, `QtQuick3D.Effects` |
| fx-material-showroom | yes | yes: "Downloading Assets..." for ever | never reaches its screen | `StudioApplication` is C++, `main.cpp` downloads the assets and calls `downloadComplete()` |

Not in the example list:

- **documentviewer** is a Qt Widgets app. Its only QML is `plugins/Q3DViewer/Viewer.qml`, which needs
  `RuntimeLoader` from `QtQuick3D.AssetUtils` and a C++ host that sets the file. It cannot run as an
  app here.
- **xr_physicsbase_teleportation** has an `XrView` as its root and needs a headset. XR stays out.
