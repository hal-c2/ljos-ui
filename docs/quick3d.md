# QtQuick3D on Ljós

QtQuick3D QML runs on Ljós natively and on the web, with `qml6` as the oracle. One wgpu renderer, `ljos_3d`, draws the scene everywhere. ARCHITECTURE.md §7.8 is the short form of this document and wins where the two differ.

Sources: [Qt Quick 3D](https://doc.qt.io/qt-6/qtquick3d-index.html), [View3D](https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html), [Node](https://doc.qt.io/qt-6/qml-qtquick3d-node.html), [Model](https://doc.qt.io/qt-6/qml-qtquick3d-model.html), [PerspectiveCamera](https://doc.qt.io/qt-6/qml-qtquick3d-perspectivecamera.html), [DirectionalLight](https://doc.qt.io/qt-6/qml-qtquick3d-directionallight.html), [DefaultMaterial](https://doc.qt.io/qt-6/qml-qtquick3d-defaultmaterial.html), [SceneEnvironment](https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html), [Qt Quick 3D coordinate system](https://doc.qt.io/qt-6/qtquick3d-coordinate-system.html). The Qt documentation only; no Qt source (CLEAN_ROOM.md).

## Goals

1. **Real QtQuick3D QML.** The types, properties, defaults and pictures of the Qt 6 module that applications use:
   - `View3D`, including as an item in a 2D scene;
   - the `Node` hierarchy;
   - `Model` with `#Cube`, `#Sphere`, `#Cylinder`, `#Cone`, `#Rectangle` and custom geometry;
   - perspective and orthographic cameras;
   - directional, point and spot lights;
   - `DefaultMaterial` and `PrincipledMaterial`;
   - `Texture`;
   - `SceneEnvironment` (clear colour, skybox and image-based light, antialiasing);
   - shadows;
   - glTF through `RuntimeLoader`;
   - picking;
   - QML animations on 3D properties.
2. **One renderer, every target.** The same crate draws natively through egui's wgpu device and on the web into a `<canvas>` (WebGPU, WebGL2 where WebGPU is missing).
3. **Beat qml6** on startup, frame time and memory for the same QML (§ Performance).
4. **A still scene costs nothing.** Nothing polls: a view draws when its scene changed or an animation runs, and is a single blit otherwise (ARCHITECTURE.md goal 4).

## Non-goals (for now)

- **2D items in 3D beyond clicks.** Items in 3D take presses and what follows them, not hover or the wheel; their pictures are CPU pixels, not a texture shared on the GPU.
- **What custom shaders cannot have yet.** `SCREEN_TEXTURE` (the scene behind a material), the lens flare of `ExtendedSceneEnvironment`, and its `ssgi*` and `ssr*` members (Qt 6.11) are left out. A snippet on the network is not read.
- **Particles3D, physics and XR.**
- **Most of `QtQuick3D.Helpers`.** Only `InstanceRange`, `RandomInstancing` and `ExtendedSceneEnvironment` are built; the others (`OrbitCameraController`, `WasdController`, `AxisHelper`, …) stand in as `Item`s, and the qtdoc corpus examples that need them stay skipped until a milestone takes them.
- **Animations in a glTF file**, so a skin's joints and a mesh's morph weights stay as the file leaves them. Joints move when QML moves them.
- **`FileInstancing`'s binary tables** (`.bin`, written by Qt's `instancer` tool): only the XML form is read. A `.bin` source warns and draws nothing.
- **`RandomInstancing.gridSpacing`,** and the custom data of an instance entry, which is kept but reaches no shader: a `CustomMaterial` has no `INSTANCE_` built-ins yet, and draws an instanced or skinned model as one unbent copy.
- **Matching Qt's pictures bit for bit.** Pixel features compare within a stated tolerance, as the 2D pictures do.
- **Opening the GL painter's 3D device on a context of its own before the first frame.** The build ends before EGL's display is up, so there is nothing to overlap it with yet (§ The painter).

## Modules and interfaces

Small interfaces, deep implementations. Each seam has one adapter on each side.

```
QML (IR) ──▶ ljos_quick3d types ──writes──▶ ljos_3d::Scene ──Arc──▶ DrawOp (View3D)
                                                                     │
               ┌─────────────────────────────────────────────────────┼──────────────────┐
               ▼                                                     ▼                  ▼
  ljos_egui::scene3d (egui-wgpu callback)      ljos_3d::Offscreen + readback   ljos_3d::SurfaceView
  shell's wgpu painter; the GL painter with    R, CPU painter, snapshots,       web <canvas>, own windows
  wgpu on its own context (gl_scene3d)         tests
               └──────────────── all draw with ljos_3d::Renderer ───────────────────────┘
```

| Crate | Owns | Interface | Depends on |
|---|---|---|---|
| `ljos_3d` | the scene (nodes, transforms, meshes, materials, cameras, lights, environment, revision), the renderer, its shaders and GPU caches | `Scene` (`add`, `remove`, `node_mut`, `set_camera`, `set_environment`, `revision`, `walk`); with `gpu`: `Renderer::{new, render, prepare_blit, blit}`, `Target`, `Offscreen`, `SurfaceView` | slotmap, thiserror; wgpu with `gpu` |
| `ljos_quick3d` (M2) | the QML types of `QtQuick3D`, their properties and defaults, the item that is `View3D` | `register(&mut Registry)`, like `ljos_lottie` | `ljos` facade, `ljos_3d` without `gpu` |
| `ljos_egui`, feature `quick3d` | the egui-wgpu adapter | `Scene3dResources::{install, retain}`, `Scene3dPaint::shape(rect)` | egui-wgpu, `ljos_3d` |
| `ljos_shell` | turning `quick3d` on; the GL painter's 3D views (wgpu on its context) | none of its own | `ljos_egui[quick3d]`, `ljos_3d`, wgpu (`gles`) |
| ljos-web | the wasm-bindgen wrapper and the canvas | its own, over `SurfaceView` | `ljos_3d` |

**Which crates may touch wgpu:** `ljos_3d` (feature `gpu`), `ljos_egui` (feature `quick3d`) and `ljos_shell` (ARCHITECTURE.md §4, crate rules). `ljos_render`'s B1 backend joins them when it is built. The QML types never see a device: they write the scene, so they run and test with no GPU.

**The IR/runtime seam.** QtQuick3D is a module of native types like any other (ARCHITECTURE.md §5.5): the compiler sees them through `ljos.txt`, and the engine runs them from IR. The scene is the 3D side's retained tree. Each 3D object owns a `NodeId` in the `Scene` of its `View3D` (or of the `View3D` whose `importScene` names it), and a property write is a write to that node, which bumps the revision. `View3D` holds the scene in an `Arc` and paints a `DrawOp` with it. On a change it makes a new `Arc` (copy on write, `Arc::make_mut`), so a frame in flight keeps the scene it was given.

**The paint seam.** `ljos_egui::scene3d` uses egui-wgpu's paint callback:
- `prepare` renders the scene into the view's `Target`, on egui's device and egui's encoder, before egui's pass begins;
- `paint` blits the target into egui's pass, where egui has set the viewport and scissor to the item.

So the 3D picture lands in paint order: items declared after the `View3D` are drawn over it and items before it under it, with no extra pass and no copy back to the CPU. A target is kept per view key and dropped by `retain` when the view goes.

**The GL seam.** The GL painter stays the default where EGL is (§ The painter), and draws 3D on its own context: `ljos_shell::gl_scene3d` hands wgpu's GL backend the painter's EGL context (`wgpu::hal::gles::Adapter::new_external`, then `Instance::create_adapter_from_hal`) the first time a view is drawn. Each frame, before egui's meshes:
- every view whose revision or size changed is drawn into its own `Target`, all in one submission;
- the view's callback becomes a mesh over its rect that samples the target's GL texture (`Texture::as_hal`), so it is clipped, blended and drawn in part like any other mesh;
- wgpu gets back the state it set when its device was opened (its vertex array, pack and unpack alignment of 1) before it works, and the painter gets its own back after (the window's framebuffer, no vertex array, no sampler objects, no pixel-unpack buffer, the default pixel store, the capabilities wgpu turns on off);
- a target whose view the whole frame no longer shows is dropped, while the context is current.

The present comparison takes a view whose scene is the same `Arc` at the same size and rect as unchanged, so a still view in a window where something else moves is not drawn again. A context wgpu cannot use (older than GL 3.3 or GLES 3) draws no 3D and says so once; it does not fall back to the read-back path, because wgpu's own EGL context would take the painter's off the thread.

**The off-screen seam.** R (vello_cpu), the processor's painter, the MCP snapshot and the BDD pictures have no wgpu pass. For them `ljos_3d::Offscreen` renders on a device of its own and reads the picture back, and the view paints it as an image. The readback happens only when the revision or size changed, so idle stays free; a view not drawn in a frame lets go of its picture at the end of it.

**The web seam.** `ljos_3d::SurfaceView` draws a scene into a surface of its own, a `<canvas>` on the web (§ The web seam (SurfaceView), below).

### The web seam (SurfaceView)

For a host with no egui and so no device to share: a web page, or a native window of an application's own. The whole interface is four calls.

| Call | Why it is there |
|---|---|
| `async SurfaceView::new(target, width, height) -> Result<_, SurfaceViewError>` | The host gives a `wgpu::SurfaceTarget` (`Canvas(HtmlCanvasElement)` or `OffscreenCanvas` on the web, a window natively) and its size in device pixels (CSS size times `devicePixelRatio`). The view makes its instance, adapter, device and surface, and sets the canvas's `width` and `height`. Asynchronous because a browser answers only through promises. |
| `backend() -> wgpu::Backend` | `BrowserWebGpu` or `Gl` (WebGL2). The choice is made inside `new`, and a host has no other way to know which it got: for its diagnostics, and to warn where WebGL2's limits matter. |
| `resize(width, height)` | Device pixels again. The host owns layout and knows when the canvas changed size; the view does not observe it. The next `draw` draws whatever the revision. |
| `draw(&Scene) -> bool` | Draws and presents if the scene's revision or the size changed since the last present; returns whether it did. |

- **Who chooses the backend.** `new` takes WebGPU when the browser has `navigator.gpu` and an adapter answers, else WebGL2 (`wgpu::util::new_instance_with_webgpu_detection`). A host that wants WebGL2 regardless hides WebGPU from the page; the interface takes no backend option until a host needs one.
- **When it draws.** Only when the host calls `draw`. The view has no frame loop, no `requestAnimationFrame` and no timer. An unchanged scene costs a comparison and no GPU call, so a host may call `draw` from every frame callback it already has, or only when its scene changed. Revisions are unique across scenes (`Scene::revision`), so handing it another scene draws.
- **What the canvas keeps.** The last presented picture, until the next `draw`. A still scene costs nothing.
- **Lost surfaces.** A surface that is lost or out of date is configured again and drawn in the same call, because a host that draws on change would not call again. A timed-out or hidden one is not drawn, and `draw` returns `false`; the next call draws it.
- **Errors** (`SurfaceViewError`, thiserror): the surface could not be made, no adapter, no device, or no format the renderer can draw into. `draw` and `resize` do not fail.
- **Colour.** The surface takes a format without the sRGB suffix (`Bgra8Unorm` or `Rgba8Unorm` on WebGPU, `Rgba8Unorm` on WebGL2) and the renderer's picture, already sRGB, is copied, not encoded again. The alpha is premultiplied where the surface offers it (WebGPU); WebGL2's canvas is premultiplied by default, so a transparent background shows the page on both.
- **No wasm-bindgen in `ljos_3d`.** The JavaScript-facing wrapper belongs to the host (ljos-web), which knows its own embedding; `ljos_3d` stays a plain Rust interface that builds for `wasm32-unknown-unknown`.

**Proven in a browser.** A harness outside the repository wraps `SurfaceView` with wasm-bindgen, draws the lit cube of `tests/spike_scene.rs` into a 400×300 canvas and checks it in headless Chromium, once on WebGPU and once with WebGPU hidden so it falls back to WebGL2. On both, on RADV (WebGL2 through ANGLE on Vulkan):
- the canvas equals the native off-screen picture (`cargo run -p ljos_3d --example lit_cube -- native.png 400 300`) exactly, at 400×300 and after a resize to 200×150; the check allows 3 per channel on all but 0.5 % of the pixels;
- an unchanged scene returns `false` and makes no WebGPU or WebGL call, counted by wrapping `GPUQueue`, `GPUCanvasContext` and `WebGL2RenderingContext`;
- a new revision of the same scene draws again.

## General purpose versus app-specific

Everything here is QtQuick3D as Qt documents it, for any application. Nothing names an application. App-specific 3D (an app's own `Geometry` subclass, its own materials) comes in through the same doors Qt has: custom geometry through a native type, written in the app's repository.

**Custom geometry (built).** `Geometry` (QtQuick3D) is the base an app's own type extends, as an app subclasses `QQuick3DGeometry` in Qt. From Rust the app calls `set_vertex_data`, `set_index_data`, `set_stride`, `add_attribute(semantic, offset, component)`, `set_primitive_type`, `set_bounds` and `clear` on the base (`Semantic` has `Index`, `Position`, `Normal`, `TexCoord0` and the rest Qt lists; `ComponentType` is `U16`, `U32` or `F32`; data is little endian), then `Geometry::update(engine, object)`, which bumps a tracked `revision` and so dirties the views that draw it (no polling). Assigning the object to `Model.geometry` draws it, instead of `source`. `ljos_3d` carries it as `MeshSource::Custom(Arc<CustomGeometry>)`: the renderer decodes and uploads each revision once, culls by the bounds (the positions are searched when none are set), and drops the buffers when no scene holds the geometry. Triangles, triangle strips and fans are drawn; a geometry with no normal attribute is lit flat. Its `TexCoord0` maps textures as a built-in mesh's UVs do, and tangents for normal maps are derived from it. Not built yet: lines and points (they need their own pipeline topology), attributes other than position, normal and `TexCoord0`, component types other than the three above, and a JS geometry API.

## Data flow

1. **Load.** The IR creates the 3D objects. `View3D` makes a `Scene`, and each `Node`, `Model`, camera and light adds its node under its QML parent.
2. **Change.** A binding, handler or animation writes a property. The type writes its node, the revision goes up, and the `View3D`'s render node is marked `CONTENT`-dirty. That schedules a repaint (ARCHITECTURE.md §7.2) and nothing else.
3. **Paint (step 8 of the frame pipeline).** `View3D` paints its `DrawOp` with the scene's `Arc` and its view key.
4. **Encode (step 11).** B0 turns the op into a paint callback.
5. **Present (step 12).** In `prepare`, a target that shows an older revision, or has the wrong size, is drawn: uniforms are written, then a depth-tested pass runs over `Scene::walk`'s visible models. `paint` blits. When nothing changed, `prepare` does nothing and `paint` is the blit.
6. **Idle.** No dirty state, no animation: no frame, no work, no wakeup.

## Testing

- **Features first** (`features/quick3d/`, ARCHITECTURE.md §18): the types' defaults, pixels at known points (clear colour, a model where the camera sees it, hidden and moved models, a transparent background), a `View3D` among 2D items, and that a still scene schedules nothing (`@ljos-only`).
  - The pixel steps read the window's picture, which for a 3D view is the off-screen readback, so they run headless.
  - The scenarios are `@backlog` until M2 and become `@qt-parity` once `qml6` has checked them.
  - Each milestone adds its files: `cameras`, `lights`, `materials`, `textures`, `environment`, `shadows`, `runtime-loader`, `picking`, `animation`.
- **The oracle.** `qml6` with QtQuick3D, offscreen, through `ljos_bdd --backend qt`. Larger pictures come from `@qt-pixels` references captured locally (git-ignored, decision D1).
  - **Missing on the reference machine:** `/usr/lib/qt6/qml/QtQuick3D` holds only `SpatialAudio`. The package `qt6-quick3d` (6.11.2) has to be installed before any scenario can become `@qt-parity`.
- **Rust tests.** `ljos_3d` unit tests cover the maths, the meshes and the scene's revisions. `ljos_3d/tests/offscreen.rs` reads back a lit cube on whatever adapter is there, and `ljos_egui/tests/scene3d.rs` composites views in an egui frame; with no adapter they print `SKIPPED`.
- **Native-type gates** for every new or changed type (`ljos_quick3d`): regenerate `ljos.txt` (`LJOS_OWN_TYPES=write cargo test -p ljos_cli --test own_types`), run the whole `own_types` test with `NOT_YET` for what is not built, commit `ljos.txt`, and run the features with `LJOS_QT_TYPES=none`.
- **Gallery.** The qtdoc QtQuick3D examples join `mise run gallery` as their milestones land (most need `QtQuick3D.Helpers`, which comes after M7).

## Performance

The bar is beating `qml6` running the same QML, on `mise run bench` (docs/BENCHMARKS.md: cgroup memory and CPU, time to the compositor mapping the window, screencopy frames).

**Measurement plan.** Synthetic apps in `bench/`, each run on both runtimes with the bench's defaults:

| App | What it stresses |
|---|---|
| `quick3d-cube.qml` | a lit `#Cube` under a camera, still: startup, settled memory, idle CPU, idle frames |
| `quick3d-spin.qml` | the same cube turned by a `NumberAnimation`: frame interval, CPU per frame |
| `quick3d-shadows.qml` | a cube turning over a floor under a shadowed sun, spot and point light: every map drawn again each frame |
| `quick3d-grid.qml` | a `Repeater3D` of 1 000 models, 4 lights, one moving: per-model cost, uniform upload |
| `quick3d-overlay.qml` | a `View3D` under Controls (a slider driving a property): composition with 2D, input to repaint |
| `quick3d-gltf.qml` (M6) | a glTF model through `RuntimeLoader`: load time, memory |
| `quick3d-instances.qml` (M10) | 10 000 `#Cube` copies from a `RandomInstancing`, their parent turning: one instanced draw, the table written once |

**Budget.** Each row must be at or below `qml6`:
- time and CPU to the first frame;
- peak and settled memory (PSS);
- p95 frame interval while animating, and CPU while animating;
- idle: 0 % CPU and 0 frames, no exceptions.

Inside Ljós the structure keeps it there:
- An unchanged view does no scene work: a revision check and one textured triangle.
- Meshes are uploaded once per primitive or geometry, and a draw uploads only uniforms. From M3, per-node uniforms stay in a retained buffer and only dirty nodes are written.
- Pipelines are built once per material kind and target format, never per draw.
- A view's target is its own size in device pixels. Nothing bigger is kept, and the target is released when the view goes.

**The painter.** The shell paints with GL by default. Drawing a `View3D` with a second (Vulkan) device next to GL's, or switching the window to egui-wgpu, was weighed by measuring the two painters on the 2D benches (§ The painter, below). egui-wgpu lost, so the GL painter draws 3D itself with wgpu on its own context: one driver, no second device, no Vulkan loader, and the NVIDIA ICD stays left out.

## The painter

Measured with `mise run bench <apps> --runtime ljos --profile desktop --warm-up --repeat 3`, `LJOS_PAINTER=gl` against `LJOS_PAINTER=wgpu`, in the headless sway sandbox. "GPU" is RADV/radeonsi on `renderD128`; "llvmpipe" is no GPU. Each figure is the median of three runs of the release build.

**2D, the benches every window runs.** Processor time while animating, and settled PSS:

| App | GPU, GL | GPU, wgpu | llvmpipe, GL | llvmpipe, wgpu |
|---|---|---|---|---|
| `animations` CPU | 2.0 % | 5.5 % | 8.4 % | 22.9 % |
| `listview` CPU | 10.4 % | 13.5 % | 9.6 % | 27.4 % |
| PSS, the seven apps | 91–117 MiB | 101–115 MiB | 114–143 MiB | 155–181 MiB |

egui-wgpu costs an animating window up to 2.75 times the processor time on a GPU and 2.9 times on llvmpipe, and 3 to 12 MiB of PSS on a GPU, 33 to 43 MiB on llvmpipe (Vulkan's loader and lavapipe or the driver's Vulkan side, mapped next to GL's). Idle is 0 % and 0 frames on both. That decided the default: GL stays.

**3D, the GL painter drawing with wgpu on its context** against egui-wgpu drawing 3D in its own pass (`bench/quick3d-cube.qml`, `bench/quick3d-spin.qml`):

| App | Measure | GPU, GL | GPU, wgpu | llvmpipe, GL | llvmpipe, wgpu |
|---|---|---|---|---|---|
| `quick3d-cube` | first frame | 60 ms | 48 ms | 67 ms | 74 ms |
| | settled PSS | 104 MiB | 115 MiB | 146 MiB | 161 MiB |
| | idle | 0 %, 0 frames | 0 %, 0 frames | 0 %, 0 frames | 0 %, 0 frames |
| `quick3d-spin` | CPU animating | 6.4 % (4.8–6.8) | 4.2 % (3.5–5.7) | 11.3 % (11.3–12.1) | 13.3 % (11.2–14.3) |
| | frames, p50 | 57 fps, 16.2 ms | 58 fps, 16.2 ms | 57 fps, 16.1 ms | 57 fps, 16.1 ms |
| | settled PSS | 102 MiB | 115 MiB | 145 MiB | 162 MiB |

- **Memory:** GL is 11 to 16 MiB lower in PSS on both. Its anonymous PSS is about 9 MiB higher on a GPU (wgpu's GL backend and the GL driver's own heap); the shared mappings it saves are larger.
- **CPU while turning:** within the spread of the runs on a GPU (the ranges overlap), and lower on llvmpipe. The 2D cost of egui-wgpu that every frame of every window pays is far larger than this.
- **First frame:** GL is 12 to 27 ms slower on a GPU, because wgpu's device on the context is opened when the first view is drawn, not at startup. On llvmpipe it is faster. Since then the device's instance has no backend of its own: the adapter is the context's, and wgpu's GL backend started an EGL display of its own for nothing, 4.7 ms of the first frame on radeonsi. A window that never shows a view no longer opens the device at all, which took about 9 ms of a 2D window's first frame and the device's memory (`Views::needs_device`).
- **Idle** is 0 % CPU and 0 frames for a still view on both painters, so the "an unchanged view is not drawn again" rule holds on GL too.

Measured on the commit that added the GL seam (`ljos_shell::gl_scene3d`); reproduce with the command above.

**Opening the device before the first frame** was measured and not done. Timed from the start of `quick3d-cube` on radeonsi:

| Step | ms |
|---|---|
| QML built | 26–40 |
| EGL display started | 41–55 |
| context current on the window | +3 to +10 |
| wgpu device on it (before: 9; now about 4.5) | +5 |
| views drawn, submitted | +6 to +12 |

The build ends before EGL's display is up, and every later step needs the one context, so there is no wait to overlap the device with. A second context opened on a thread while the QML builds could only pay off where the build outlasts EGL's start, and its configuration would have to match the window's (on X11 too). The startup bench (`mise run bench quick3d-cube rectangles --runtime ljos --profile desktop --warm-up --repeat 3`) ran at a load of 60 to 85, too busy to show a change of 5 ms: the fastest first frames were 52 → 53 ms for `quick3d-cube` and 53 → 53 ms for `rectangles`, before → after.

**GPU time.** Where the device can read timestamp queries back without waiting, each target times its shadow passes and its scene pass (which shades, resolves the samples and tonemaps), and `app_metrics` reports it as `paint.gpu_3d_us` and the line "drawing 3D on the GPU". Release build, sandbox on radeonsi and RADV, 10 s of each app turning, last and longest of the readings, and the mean over the frames:

| App | GL painter (GL) | wgpu painter (Vulkan) |
|---|---|---|
| `quick3d-spin` | 20 µs, longest 22, mean 21 | 28 µs, longest 31, mean 28 |
| `quick3d-shadows` | 526 µs, longest 537, mean 515 | 245 µs, longest 259, mean 239 |
| `quick3d-grid` | 135 µs, longest 1632, mean 247 | 413 µs, longest 435, mean 403 |

The three shadowed lights cost 0.2 to 0.5 ms of GPU a frame, and the 1000 cubes 0.1 to 0.4 ms; both are far inside a 16.7 ms frame, so these benches are bound by the processor, not the GPU. Reproduce with `ljos run --mcp bench/<app>.qml` and `app_metrics`.

## Milestones

| # | Builds | Exit criteria |
|---|---|---|
| **M1 Spike** (done) | `ljos_3d` scene and renderer (one directional light, `#Cube`, `#Rectangle`, DefaultMaterial diffuse, perspective and orthographic projection); `Offscreen`; `SurfaceView`; the egui-wgpu adapter | a lit cube renders off-screen and identically on Vulkan, GL and llvmpipe; it composites in an egui frame between 2D shapes, two views in one frame; `cargo check --target wasm32-unknown-unknown -p ljos_3d` passes |
| **M2 View3D on screen** | `ljos_quick3d` with `View3D`, `Node`, `Model` (`#Cube`, `#Rectangle`), `PerspectiveCamera`, `DirectionalLight`, `DefaultMaterial` (diffuse, `lighting`), `SceneEnvironment` (`clearColor`, `backgroundMode`); the `View3D` draw op in `ljos_render`; B0 callback; the shell's painter choice; the readback path for R and tests | `features/quick3d/view3d*.feature` off the backlog and `@qt-parity` on qml6; native-type gates green; `quick3d-cube` idle at 0 % and 0 frames; `ljos run` shows the cube in the desktop sandbox |
| **M3 Shapes, cameras, lights, PBR** | `#Sphere`, `#Cylinder`, `#Cone`; `OrthographicCamera`, `frustumCulling`; point and spot lights with Qt's attenuation, many lights; `PrincipledMaterial` (metal-roughness, emissive, opacity, alpha modes); MSAA (`antialiasingMode`); custom geometry (done); retained per-node uniforms | `cameras`, `lights` and `materials` features `@qt-parity` within tolerance; `quick3d-grid` frame interval ≤ qml6 |
| **M4 Textures and environment** (built; parity waits for qml6 with QtQuick3D) | `Texture` (every property; `sourceItem` later), DefaultMaterial `diffuseMap`, PrincipledMaterial maps with channels, tangents; skybox, `lightProbe` IBL prefiltered on the GPU, `.hdr` images, tonemapping modes | `textures`, `texture-maps` and `environment` features `@qt-parity`; memory ≤ qml6 with a 2k HDR probe |
| **M5 Shadows** (built; Qt check pending) | directional, point and spot shadow maps; `castsShadow`, `shadowFactor`, `shadowBias`, `shadowMapQuality`, `shadowMapFar`, `softShadowQuality`, `pcfFactor`, the cascades (`csmNumSplits`, `csmSplit1`–`3`, `csmBlendRatio`, `lockShadowmapTexels`); a model's `castsShadows` and `receivesShadows`; 16 lights, 4 of them shadowed | `shadows` features within tolerance of qml6 pictures. Built: the `shadows` feature passes on Ljós, not yet checked on qml6 (no `qt6-quick3d`); `quick3d-shadows` (three shadowed lights, the caster turning) shows 57 frames/s at 7.3 % of a core, against 4.4 % for `quick3d-spin`; the shadow passes add 0.15–0.3 ms a frame, and the GPU takes 0.25 ms (Vulkan) to 0.53 ms (GL) for the frame's 3D (§ The painter) |
| **M6 glTF** (built; Qt check pending) | `RuntimeLoader` (`source`, `status`, `errorString`, `bounds`), parsed on the compute pool, meshes, PBR materials and textures | `runtime-loader` features; `quick3d-gltf` load time and memory ≤ qml6. Built: `.gltf` with its files and `.glb`, the node tree (TRS and matrices), meshes (positions, normals, two UV sets, tangents, indices; triangles, strips, fans), metal-roughness materials (base colour, metalness, roughness, emissive, alpha mode and cutoff, double-sided, unlit) with their base colour, metal-roughness, normal, occlusion and emissive textures as M4's maps. The `runtime-loader` features pass on Ljós, not yet checked on qml6. `quick3d-gltf` (nine spheres, 5184 triangles): first frame 47.5 ms, settled PSS 106 MiB (33.3 MiB private), idle 0.00 % and 0 wakeups; the pool's parse and build take 16 µs for it and 3.9 ms for a 262 144-triangle, 6.3 MB GLB (`cargo run --release -p ljos_3d --features gltf --example gltf_load -- FILE`) |
| **M7 Picking and animation** (built; Qt check pending) | `View3D.pick`, `pickAll`, `rayPick`, `mapFrom3DScene` and `mapTo3DScene`; animations on `vector3d`/`quaternion` properties; `Repeater3D` | `picking` and `animation` features `@qt-parity`; `quick3d-spin` frame interval and CPU ≤ qml6. Built: `picking`, `picking-rays`, `picking-cameras`, `mapping`, `animation` and `repeater3d` features pass on Ljós, not yet checked on qml6 (no `qt6-quick3d`); `quick3d-grid` (1000 cubes, 4 lights, one moving) shows 50 frames/s at 17 % of a core, 0.1–0.4 ms of it on the GPU |
| **M8 Web** | `SurfaceView` as the web seam (done, § The web seam (SurfaceView)); ljos-web draws through it into a `<canvas>`; the WebGL2 fallback checked | the same scenes render in a browser with WebGPU and with WebGL2: done for the lit cube in a harness, equal to the native picture on both; ljos-web's embedding remains |
| **M9 2D items in 3D** (built; Qt check pending) | `Texture.sourceItem` drawn live; an `Item` declared in a `Node` drawn in its plane; presses to both through the pick | `source-item` and `items-in-3d` features. Built: both pass on Ljós (18 scenarios), not yet checked on qml6 (no `qt6-quick3d`); an item that does not change is not drawn again and leaves no frame scheduled |
| **M10 Instancing, skins, morphs** (built; Qt check pending) | `Instancing` on `Model` and `RuntimeLoader` (`InstanceList` and `InstanceListEntry`, `FileInstancing` from XML, `instanceRoot`, `depthSortingEnabled`, `hasTransparency`, `instanceCountOverride`); `InstanceRange` and `RandomInstancing` from `QtQuick3D.Helpers`; `PickResult.instanceIndex`; `Skin`, `Skeleton` and `Joint`, and a glTF file's skins, drawn on the GPU; a glTF file's morph targets by the node's or mesh's weights; the second UV set (`Texture.indexUV`, glTF `texCoord` 1); the children of a loaded file pickable | `instancing`, `instance-tables`, `skinning`, `morph-targets`, `uv-sets` and `runtime-loader-picking` features pass on Ljós, not yet checked on qml6; `quick3d-instances` (10 000 copies) shows its first frame in 115 ms and 53.7 frames/s at 10.8 % of a core, p95 32.4 ms, 38.5 MiB settled private memory, against 126 ms, 56.4 frames/s, 6.7 %, 32.4 ms and 34.7 MiB for `quick3d-spin` on the same run (median of 3, release, load average 19–33); the Khronos samples `SimpleSkin`, `RiggedSimple`, `SimpleMorph` and `AnimatedMorphCube` load and draw (`crates/ljos_3d/tests/exported_assets.rs`) |
| **M11 Custom shaders and post-processing** (built; Qt check pending) | `CustomMaterial` (unshaded and shaded, light-processor functions, `TextureInput`, blending, `depthDrawMode`), `Effect` (`Pass`, `Shader`, `Buffer`, `BufferInput`, `SetUniformValue`, `INPUT`/`OUTPUT`/`DEPTH_TEXTURE`), `ExtendedSceneEnvironment` (exposure, white point, glow and bloom, depth of field, colour adjustments, vignette, LUT, sharpening, FXAA, dithering), SSAO from `SceneEnvironment.ao*` | `custom-material`, `effects`, `extended-environment` and `ambient-occlusion` features: 40 scenarios pass on Ljós, not yet checked on qml6 (no `qt6-quick3d`); a still scene runs no pass |
| **Later** | the rest of Helpers | gallery's QtQuick3D examples render |

## Spike (M1): the shared device and wasm

The riskiest seam was rendering 3D with egui's own wgpu device and queue and compositing it into the 2D scene in paint order, from a crate that also builds for wasm.

- `ljos_3d` renders a `View3D` of a lit `#Cube` under a `PerspectiveCamera` (the QML is in `crates/ljos_3d/tests/spike_scene.rs`) off-screen and reads it back.
  - It runs on Vulkan (RADV), GL (radeonsi) and llvmpipe; the three pictures are identical.
  - `tests/offscreen.rs` checks the clear colour at the corners, three distinctly lit faces, a transparent background, and that an unchanged revision is not drawn again.
- `ljos_egui/tests/scene3d.rs` paints red under a view in a real egui-wgpu frame. Red shows where the cube is not, and every opaque pixel of the cube equals the off-screen reference exactly. Two views in one frame keep their own scenes, which is why uniforms are per target and not shared.
- `cargo check --target wasm32-unknown-unknown -p ljos_3d` passes with and without `gpu`, `SurfaceView` included.

## Decisions

- **Name and split.** `ljos_3d` holds the scene and renderer, with a `gpu` feature. `ljos_quick3d` holds the QML types, without `gpu`.
- **Render to a texture in `prepare`, blit in `paint`.** The scene gets its own depth buffer and clear, independent of egui's pass. The cost is one target per view.
- **Per-target uniforms.** `queue.write_buffer` lands before the submission, so views that shared one buffer would all see the last view's values.
- **The egui adapter lives in `ljos_egui`, behind `quick3d`.** It names egui types, so it stays inside the three egui crates.
- **Colour.** An `Rgba8Unorm` target, encoded to sRGB in the shader, premultiplied, so the blit matches egui's gamma-space pass with no conversion.
- **Painter choice.** GL stays the default and draws 3D with wgpu on its own EGL context; egui-wgpu draws 3D in its own pass where it is the painter (`LJOS_PAINTER=wgpu`, or no EGL). Readback for R, the processor's painter, snapshots and tests. The numbers are in § The painter.
- **Own small maths** instead of glam: a few dozen lines, no dependency, column-major and matching WGSL.
- **wgpu 30.0.1,** the version egui-wgpu 0.37 is built on, with backends chosen per target.
- **The GL painter opens wgpu with the first frame that shows a view,** on its own context, through an instance without backends (`create_adapter_from_hal` only wraps the adapter). A program without a `View3D` never opens it. Opening it earlier is not done: § The painter has the timeline.
- **GPU timing only where it cannot stall.** `ljos_3d::timing_features` asks for `TIMESTAMP_QUERY` only where the adapter has it and resolves queries without the processor (`DownlevelFlags::NONBLOCKING_QUERY_RESOLVE`), and never on WebGL2. Both painters ask for it. Each target has four queries: the first shadow pass's start and the last one's end, then the scene pass's. A frame resolves only what it wrote into one of three buffers, which is mapped when the target is next drawn and read at the drawing after that. wgpu completes the mapping at a later submit, so nothing waits or polls, and a reading is one or two drawings late; a frame that finds all three buffers busy is drawn untimed. The IBL prefilter is not timed, because it runs once per probe. `Target::last_gpu_time` keeps the last reading, and `take_gpu_time` hands each reading over once, which is what the paint report counts.

- **Picking is on the CPU** (`ljos_3d::pick`, no `gpu` needed, so it builds for wasm). It reads the scene the view draws, untracked, and goes through the camera's own projection, so a model is picked where it is drawn. A world bounding sphere rejects most models before the triangles are tested. Hits are on the mesh's triangles, with interpolated normals and UVs, where Qt's documentation speaks of bounding volumes. `pickable` stays a QML property, given to the pick as a filter.
- **A node's parent is `Object3D.parent`:** the object it is declared in until `parent` is set, declared or later, which moves it into the new parent's `data` (where the scene is walked) and out of the old one's. A node set inside itself is refused with a warning. A node declared in a `View3D` itself has an empty `parent`, where Qt's names the scene's root node, which no QML declares; setting `parent` to null takes a node out of the scene (`features/quick3d/node-parent.feature`). Its own methods are called by name inside the view, `pick(x, y)` as Qt's scope allows (`picking-unqualified.feature`). Scene properties (`scenePosition`, `forward`, …) are bound when first read, so nodes nobody asks pay nothing. A camera outside any `View3D` maps through a viewport one unit square. `lookAt` writes `eulerRotation`, keeping +Y up.
- **`Repeater3D` puts its nodes in its own `data`,** not its parent's as `Repeater` does: it is a node, and its nodes move with it.
- **`QuaternionAnimation` lives in `ljos_quick`** beside the animations it runs with; `ljos_quick3d` registers it through the facade. Its Euler ends compose as `eulerRotation` does (Z, then X, then Y). An animation declared `on` a member of a value (`eulerRotation.y`, `font.pixelSize`) moves the member, for any module.
- **Textures load when first drawn.** A small local picture (≤ 512² by its header) is decoded on the spot, as an `Image`'s is, so the first frame has it. Others are read in the background and the frame waits for them, as Qt reads a texture before it draws. A view that drew without the picture is painted again when it arrives, so nothing polls. A picture that cannot be read is left out until the source changes.
- **One upload a picture.** The renderer keys uploads by the picture's id and lets them go when no scene holds the picture, so one picture on many materials and views is uploaded once. The decoded pixels are let go after the upload; mipmaps are made on the CPU.
- **Probes.** A light probe (Radiance or an ordinary picture) becomes a cube of up to 512 a side, prefiltered once on the GPU into six specular levels and a 32² irradiance cube. The specular response uses Karis's analytic environment BRDF, not a lookup table. `probeHorizon` darkens the lower half in world space. The pixels are let go once uploaded.
- **Half-float scenes, tonemapped at the end.** Lit scenes are drawn in `Rgba16Float` and tonemapped (none, linear, ACES, Hejl-Dawson, filmic) as they are resolved. A scene with effects, ambient occlusion or an `ExtendedSceneEnvironment` stays in half floats untonemapped through its passes, and the last pass tonemaps, grades and dithers it, with FXAA after. A target that cannot render half floats (WebGL2 without `EXT_color_buffer_float`) draws without a probe; hosts pass the adapter so the renderer knows.
- **Tonemap defaults.** The default is Linear, as the documentation says. `TonemapModeNone` writes light as it is, ignoring exposure and `probeExposure`. `TonemapModeCustom` draws as Linear. Exposure and white point come from `ExtendedSceneEnvironment`; a plain `SceneEnvironment` draws with both at 1.
- **Sky box fallbacks.** `SkyBox` without a probe clears to transparent; `SkyBoxCubeMap` and `SkyMaterial` draw as Transparent.
- **Shadow maps.** Shadow settings live on the light's `Node` (`Node::shadow`), next to `casts_shadows` and `receives_shadows`, so the scene stays one tree of plain data.
  - One `Depth32Float` 2D array per target holds every layer: one for a spot light, six for a point light (a cube's faces, drawn as six 90° perspectives), one per cascade for a directional light. The array and its comparison sampler go in the frame's bind group, because WebGL2 allows four groups.
  - At most 16 lights, and the first 4 that cast shadows get maps (24 layers). The maps share one side, the largest asked for, halved until the array fits in 64 MiB, never below 256.
  - An array of a multiple of six square layers is taken for cube maps by wgpu (and by GL), and its layers then read back as nothing; the array gets one layer more. GL also takes an array of one layer for a plain texture, so there are at least two.
  - Without cascades a directional light's map covers every model that casts or receives shadows, wherever the camera is; with them, each cascade covers its slice of the camera's view out to `shadowMapFar`, blended into the next over `csmBlendRatio`.
  - `shadowBias` moves the surface towards the light, in scene units, before it is compared. `softShadowQuality` takes 1 to 16 samples on a disk `pcfFactor` across; PCF32 and PCF64 take 16. `shadowFilter` and `use32BitShadowmap` are kept without effect.
  - Casters are drawn with their material's cull mode. Blended and masked materials cast solid shadows.
  - The maps are drawn again only when a shadowed light or a caster changes: a camera that moves (without cascades) reuses them.
- **2D items are pictures drawn by the 2D painter.** `ljos_quick::item_picture` lets a reader (the view) ask for a picture of an item. The scene that shows the reader adopts the item's tree, under a hidden node when it is not in the window, so it is laid out and painted as any item, and draws it off the screen on the CPU at the scene's scale (`compose_subtree_in`, the CPU reference painter). It draws it again only when the frame's fingerprint changes, then repaints whoever read it. The first frame lacks the picture; the next has it. The pixels go to `ljos_3d` as an ordinary `TexturePixels`, so every painter and the web draw it with no new renderer code.
  - `sourceItem` shows the item's own bounds; `source` is then ignored, as documented.
  - An `Item` declared in a `Node` is moved under an item of no size that the node holds, so `anchors.centerIn: parent` centres it on the origin. The holder's children are one picture of the rectangle they cover, shown on an unlit, two-sided rectangle one scene unit a pixel, whose top-left corner is where the items' is, with y turned up. These rectangles cast and receive no shadows.
  - **Presses through the pick.** `View3D` is a pointer portal (`ljos_quick::register_pointer_portal`). A press nothing above the view takes goes to the item the nearest hit shows: a node's items where their rectangle is hit, or the `sourceItem` of a pickable model's first material's colour map (`diffuseMap`, `baseColorMap`) at the texture's own UV transform and tiling. The item's tree is hit-tested from the items themselves, topmost first, and the moves and the release follow through the same mapping while the press is held; off the model they land off every item, so a release there is no click. `pick` still reports models only (`hitType` stays Model).
- **Custom shaders are GLSL snippets translated to WGSL by naga** (`ljos_3d::snippet`), wrapped with the documented keywords, when a view first draws them. No new crate: naga is wgpu's.
  - A snippet file is read synchronously when it is a resource or a local file; one on the network is refused with a warning. A program is cached on its object by the URLs, shading mode and uniform layout; a failure (unreadable, not compiling, unsupported) is warned once, naming the file, and the model or effect draws nothing.
  - A material's or effect's properties past the native ones are its uniforms: a colour made linear, a `quaternion` as (x, y, z, scalar); `var` and `string` are skipped. A `TextureInput` property is a sampler; a disabled one samples opaque black. At most 8 textures and 8 `VARYING`s.
  - A pass without a fragment shader copies its input. A `BufferInput` with no sampler is the pass's `INPUT`; one naming a sampler that a `TextureInput` property also names replaces it. `SetUniformValue` changes a uniform for its own pass.
  - `aoEnabled` (6.5) decides once it is authored; otherwise `aoStrength` alone does, as before 6.5. Turning it on gives a zero strength 100 and a zero distance 5.
  - `Material.depthDrawMode` is on every material; the built-in materials ignore it.
  - Kept without effect: `glowUseBicubicUpscale` (levels are scaled up linearly), `sourceAlphaBlend`, `destinationAlphaBlend`, `alwaysDirty`, `lineWidth`, `Buffer.bufferFlags` and the lens flare members.
  - An author's program reads one copy and no joints: a `CustomMaterial` on an instanced or skinned model draws, casts and is culled as the model drawn once and unbent, as its vertex stage has no `INSTANCE_` or skinning built-ins yet.
- **Kept without effect:** `mappingMode`, `tilingModeDepth` and `autoOrientation`.

- **glTF through the `gltf` crate** (1.4.1, default features off but `names` and `KHR_materials_unlit`; no networking, no image crate, its own base64 for `data:` URIs). PNG and JPEG are decoded with `png` and `zune-jpeg`, already in the workspace. All of it is behind `ljos_3d`'s `gltf` feature, which `ljos_quick3d` turns on by default; without it every `source` is an error that says so.
- **A loaded model goes through `MeshSource::Custom`,** one `CustomGeometry` per primitive, interleaved F32 (position, normal, UV0, UV1, tangent) with u32 indices: no second upload path. Its nodes are added under the loader's scene node at each walk, so they cannot be bound to; each mesh primitive is a pickable model, and a pick's `objectHit` is a `Model` the loader owns, named after the file's node, its transform the identity (where it is placed is the file's business, not the object's).
- **Textures are decoded on the pool into plain RGBA8** (`ljos_3d::gltf::MaterialTextures` beside each material, with UV set and sampler). `ljos_3d::gltf::scene_material` makes them the material's maps: each factor times its texture, roughness from green and metalness from blue (the maps' default channels), `normalTexture.scale` as the normal strength and `occlusionTexture.strength` as the occlusion amount. glTF lays UVs out with the image's top at v = 0, as the built-in meshes do, so nothing is flipped. A picture is one `TexturePixels` shared by every material using it, so it is uploaded once; its id comes from `fresh_pixels_id`, the top half of the id space, which an image cache's ids never reach. The pixels stay after the upload, because the model is re-added at every walk and another view or renderer may draw it later. A texture names its UV set, and the renderer reads the first two; past the second, the second. Images are at most 8192 a side.
- **What is refused.** Points and lines are skipped with a warning, as are sparse accessors. A buffer that cannot be had is an error; an image that cannot be had or decoded is a warning and its material keeps its factors. A node with two parents or in a cycle is an error. The roots are the default scene's, else the first scene's, else the nodes no node holds. A matrix is taken apart into translation, rotation and scale, so shear is lost. One glTF unit is one scene unit.
- **`status` stays Empty while a file loads:** Qt names no Loading value. A source's files are found as any source is (resource, local path, fetch), relative URIs against the glTF's own URL; remote files are fetched one at a time. `bounds` is a map of `minimum` and `maximum` (`vector3d`), zero when nothing is loaded. `instancing` draws every loaded model by the table.
- **Instance rows.** A copy is 64 bytes: the top three rows of its matrix and its linear colour, a per-instance vertex buffer at locations 4–7. Every model is drawn from such a range; one drawn once reads row 0, the identity, so one pipeline draws both. The rows are written only from the first that changed, so a still table costs no upload. `depthSortingEnabled` sorts a model's copies far to near among themselves; `hasTransparency` blends them by their colour's alpha. Picking tests each copy and gives its row as `instanceIndex`, −1 for a model that is not instanced.
- **The second vertex buffer.** A mesh with a second UV set or joints gets a second buffer (UV1, joints as `Uint16x4`, weights) at locations 8–10, drawn by `vs_extra`; other meshes pay nothing.
- **Skins on the GPU.** A skin's palette (joint world matrix times inverse bind) is at most 256 joints (`MAX_JOINTS`), a slot of a per-target uniform buffer bound at a dynamic offset beside the model's uniforms (WebGL2 has no storage buffers). Picking reads the same bent pose, so a skinned model is picked where it is drawn. In QML, a skin naming a joint that is not in the scene is drawn unskinned rather than torn; in a loaded file, such a joint stays where the loader is.
- **Morph targets are applied when the file is built,** by the node's weights, else the mesh's, the targets' normal and tangent deltas added the same way and the normals made unit length again. They do not change after, as nothing in QML or the file (animations being left out) changes them.
- **`InstanceRange.proportional` defaults to false.** The documentation's text says true, but its examples (a position range from one corner to another filling a box) only work with each component picked on its own; following the text puts every copy on one line. `randomSeed` −1 is random, picked once per change of the inputs, so the table does not churn while nothing changes. Quaternion ranges are x, y, z, w; colour ranges are picked in the colour model.
- **`QtQuick3D.Helpers` built in part.** The module is registered with the two types Ljós builds; the compiler marks the Qt types the own table exports as built (`Type::is_built`, `TypeOracle::is_built`), and in a module that stands in, a built type resolves as itself while the rest still stand in as `Item`. A partly built module raises no stand-in warning for its built types.

## Open risks

- **Unchecked M9 conventions:** the snippet keywords' exact meaning (`BASE_COLOR` against the light functions, `EMISSIVE_COLOR`'s tonemapping), the glow levels' weights and blend modes, the vignette, depth-of-field and SSAO kernels, and the enum numbers come from the documentation, not qml6. The features' pixels wait for QtQuick3D on the reference machine.
- **GLSL beyond the features' snippets.** The translator is checked on the features' snippets and the unit tests, not on the qtdoc examples' shaders; an unsupported construct is a warning naming the file, and the model draws nothing.
- **Post-processing cost on slow GPUs.** Each effect pass, the depth pass, SSAO, glow's levels and DoF are full-screen draws on every frame the scene changes; unmeasured on the Pi Zero profile and WebGL2.

- **GL state shared with wgpu.** The GL painter and wgpu take turns on one context. What each sets for the other is listed in § The GL seam; a later wgpu that keeps more state from device creation, or a painter change that leaves more bound, shows as a broken frame, not an error. The desktop sandbox pictures (`cua-gl.png` on RADV and llvmpipe) are the check.
- **No oracle yet:** `qt6-quick3d` is not installed. Conventions taken from the documentation are not checked against qml6:
  - which way lights point;
  - the order euler rotations apply in (the spike applies z, then x, then y);
  - the default tonemapping;
  - the strength of ambient light.
- **Target lifetime** is handled on every path: egui-wgpu's `retain` and the GL painter keep the targets of the views the frame shows, and the read-back pictures keep those drawn in the frame. Each path has an automated test:
  - `ljos_quick3d`'s `a_destroyed_view_lets_go_of_its_picture` checks the read-back path through twenty Loader rounds.
  - `ljos_egui`'s `views_that_come_and_go_leave_no_pictures_behind` checks egui-wgpu's: views come and go for fifty rounds, and wgpu's count of live textures comes back to where it was.
  - `ljos_shell`'s `views_that_come_and_go_leave_no_gl_textures_behind` checks the GL painter's, on every EGL device without a window (radeonsi and llvmpipe here), and prints SKIPPED where EGL has none. It found that wgpu frees textures only at a submit or poll, so the GL textures of the last views gone outlived them: the painter now polls once, without waiting, when it lets pictures go, and when it is dropped.
- **MSAA and the blit:** resolving before the blit keeps composition unchanged. The cost is to be measured at M3.
- **Unchecked M7 conventions:** whether Qt's `pick` hits triangles or bounding boxes, `PickResult.uvPosition` for primitives, and `mapFrom3DScene`'s z (taken as the distance past the near plane) wait for qml6 with QtQuick3D.
- **The web wrapper's owner** is ljos-web. Its embedding API has to agree with `SurfaceView`.
- **The web after M3.** The harness now also draws a scene with MSAA 4×, PBR spheres under a directional and a point light, a textured cube and a blended sphere, one lit by a Radiance light probe shown as its sky box, one shadowed by a directional light with two cascades, a hard spot light and a soft point light, and two glTF files read by the loader, one with an unlit four-colour texture and one lit with a checker texture. All four match the native picture exactly on WebGPU and on WebGL2 (ANGLE on Vulkan), as the lit cube does, and an unchanged scene makes no GPU calls. What stays unchecked: a WebGL2 without `EXT_color_buffer_float` (the renderer then draws without the probe) and other browsers and GPUs.
- **M4 numbers** (RADV, 2048×1024 probe): its maps hold 16,822,272 bytes on the GPU. The prefilter frame takes about 48 ms the first time and 4 ms after (22 and 2 ms on GL); the transient peak is about 17 MB for the environment cube plus 8 MB for the source. `mise run bench` on a probe scene settles at 30.3 MiB private PSS, against 24.7 for `quick3d-cube`, with 0 % idle CPU. The bench is not in `bench/` because it needs an 8 MB `.hdr` beside it. Whether this is at most what qml6 uses waits for QtQuick3D on the reference machine.
- **Unchecked M5 conventions:** how dark `shadowFactor` makes a shadow (taken as the share of the light's direct radiance it removes), the bias in scene units, the PCF disk, and how far cascades reach all come from the documentation, not qml6. The `shadows` feature's pixels wait for QtQuick3D on the reference machine.
- **Shadow cost on slow GPUs.** Every shadowed point light is six extra passes over its casters, and a moving caster redraws every map. On the desktop this is 0.15–0.3 ms a frame for three lights; on the Pi Zero profile and WebGL2 it is unmeasured (WebGL2 draws the same shadows, in 59 calls for three shadowed lights).
- **Probe orientation and the env BRDF** follow the documentation and common practice, not qml6; the exact blur of `skyboxBlurAmount` and the Hejl-Dawson and filmic constants are the published ones, which Qt may tune differently.
- **2D items in 3D:**
  - The picture is cut to the item's bounds (`sourceItem`) or to its children's rectangle (a node's items), so a child drawn outside is clipped. Every change redraws the whole picture on the CPU, which costs a large or busy item more than a GPU-shared texture would.
  - One area per item: a node's holder and a `sourceItem` are separate items, but the same item used as two textures with different areas takes the last.
  - Whether Qt flips a `sourceItem` texture, what a node's items are lit by and how a press is mapped all come from the documentation, not qml6.
  - The desktop sandbox lost the first click after launch on both painters; later clicks all landed. Not seen in the features.
- **glTF beyond the test files.** The loader is checked on its own small files and fuzzed (prefixes, digit and byte mutations, 4000 rounds a run, 300 000 once), not on exporters' output. Skins and morph targets are also checked on four Khronos sample files (CC0, fetched to a scratch directory and named by `LJOS_3D_ASSETS`; the test says SKIPPED without them, so CI does not run it). Until a KTX2/Draco/meshopt extension is wanted, files that need one fail to parse with the extension named. Animations and cameras in the file are ignored.
- **Unchecked M10 conventions:** whether Qt's `instanceRoot` keeps the space between root and model beside each copy, how `depthSortingEnabled` orders copies against other models, the `proportional` default and `randomSeed` −1, and the bent pose a skinned pick hits all come from the documentation, not qml6.
- **The compiler's partly built modules.** `is_built` changes how `qml_front` resolves every stand-in module, not only Helpers; a Qt type wrongly listed as exported by the own table would resolve as itself and lose its stand-in. The `module-stand-in` and `check-without-qt` features guard the rule.
- **Instanced cost.** 10 000 copies take 4 points of a core more than one spinning cube (10.8 % against 6.7 %), the depth sort, where asked for, being on the CPU. A table that changes every frame is rewritten from its first changed row; tables much larger than the bench's are unmeasured, as are WebGL2 and the Pi Zero profile.
