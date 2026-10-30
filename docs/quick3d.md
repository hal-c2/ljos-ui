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

- **2D items inside 3D** (an `Item` child of a `Node`, `Texture.sourceItem`). Later, on top of the texture path of M4.
- **`CustomMaterial`, `Effect` and `ExtendedSceneEnvironment`.** Later: they need Qt's shader snippets translated, which `ljos_shader` is the start of.
- **Particles3D, physics, XR, instancing tables, morph targets and `QtQuick3D.Helpers`.** The qtdoc corpus examples that need Helpers stay skipped until a milestone takes them.
- **Matching Qt's pictures bit for bit.** Pixel features compare within a stated tolerance, as the 2D pictures do.

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

Everything here is QtQuick3D as Qt documents it, for any application. Nothing names an application. App-specific 3D (an app's own `Geometry` subclass, its own materials) comes in through the same doors Qt has: custom geometry through a native type or the JS geometry API that M3 defines, written in the app's repository.

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
| `quick3d-grid.qml` | a `Repeater3D` of 1 000 models, 4 lights, one moving: per-model cost, uniform upload |
| `quick3d-overlay.qml` | a `View3D` under Controls (a slider driving a property): composition with 2D, input to repaint |
| `quick3d-gltf.qml` (M6) | a glTF model through `RuntimeLoader`: load time, memory |

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
- **First frame:** GL is 12 to 27 ms slower on a GPU, because wgpu's device on the context is opened when the first view is drawn, not at startup. On llvmpipe it is faster. Opening the device before the first frame where the QML has a `View3D` is a later improvement.
- **Idle** is 0 % CPU and 0 frames for a still view on both painters, so the "an unchanged view is not drawn again" rule holds on GL too.

Measured on the commit that added the GL seam (`ljos_shell::gl_scene3d`); reproduce with the command above.

## Milestones

| # | Builds | Exit criteria |
|---|---|---|
| **M1 Spike** (done) | `ljos_3d` scene and renderer (one directional light, `#Cube`, `#Rectangle`, DefaultMaterial diffuse, perspective and orthographic projection); `Offscreen`; `SurfaceView`; the egui-wgpu adapter | a lit cube renders off-screen and identically on Vulkan, GL and llvmpipe; it composites in an egui frame between 2D shapes, two views in one frame; `cargo check --target wasm32-unknown-unknown -p ljos_3d` passes |
| **M2 View3D on screen** | `ljos_quick3d` with `View3D`, `Node`, `Model` (`#Cube`, `#Rectangle`), `PerspectiveCamera`, `DirectionalLight`, `DefaultMaterial` (diffuse, `lighting`), `SceneEnvironment` (`clearColor`, `backgroundMode`); the `View3D` draw op in `ljos_render`; B0 callback; the shell's painter choice; the readback path for R and tests | `features/quick3d/view3d*.feature` off the backlog and `@qt-parity` on qml6; native-type gates green; `quick3d-cube` idle at 0 % and 0 frames; `ljos run` shows the cube in the desktop sandbox |
| **M3 Shapes, cameras, lights, PBR** | `#Sphere`, `#Cylinder`, `#Cone`; `OrthographicCamera`, `frustumCulling`; point and spot lights with Qt's attenuation, many lights; `PrincipledMaterial` (metal-roughness, emissive, opacity, alpha modes); MSAA (`antialiasingMode`); custom geometry; retained per-node uniforms | `cameras`, `lights` and `materials` features `@qt-parity` within tolerance; `quick3d-grid` frame interval ≤ qml6 |
| **M4 Textures and environment** | `Texture` (`source`, sampling, `Image` sources), skybox, `lightProbe` IBL, HDR images, tonemapping modes | `textures` and `environment` features `@qt-parity`; memory ≤ qml6 with a 2k HDR probe |
| **M5 Shadows** | directional, point and spot shadow maps; `castsShadow`, `receivesShadow`, `shadowMapQuality`, `shadowBias` | `shadows` features within tolerance of qml6 pictures |
| **M6 glTF** | `RuntimeLoader` (`source`, `status`, `errorString`, `bounds`), parsed on the compute pool, meshes, PBR materials and textures | `runtime-loader` features; `quick3d-gltf` load time and memory ≤ qml6 |
| **M7 Picking and animation** | `View3D.pick`, `pickAll`, `rayPick`, `mapFrom3DScene` and `mapTo3DScene`; animations on `vector3d`/`quaternion` properties; `Repeater3D` | `picking` and `animation` features `@qt-parity`; `quick3d-spin` frame interval and CPU ≤ qml6 |
| **M8 Web** | `SurfaceView` as the web seam (done, § The web seam (SurfaceView)); ljos-web draws through it into a `<canvas>`; the WebGL2 fallback checked | the same scenes render in a browser with WebGPU and with WebGL2: done for the lit cube in a harness, equal to the native picture on both; ljos-web's embedding remains |
| **Later** | 2D items in 3D, `CustomMaterial`, effects, Helpers | gallery's QtQuick3D examples render |

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

## Open risks

- **GL state shared with wgpu.** The GL painter and wgpu take turns on one context. What each sets for the other is listed in § The GL seam; a later wgpu that keeps more state from device creation, or a painter change that leaves more bound, shows as a broken frame, not an error. The desktop sandbox pictures (`cua-gl.png` on RADV and llvmpipe) are the check.
- **No oracle yet:** `qt6-quick3d` is not installed. Conventions taken from the documentation are not checked against qml6:
  - which way lights point;
  - the order euler rotations apply in (the spike applies z, then x, then y);
  - the default tonemapping;
  - the strength of ambient light.
- **Target lifetime** is handled on every path: egui-wgpu's `retain` and the GL painter keep the targets of the views the frame shows, and the read-back pictures keep those drawn in the frame. `ljos_quick3d`'s `a_destroyed_view_lets_go_of_its_picture` checks the read-back path through twenty Loader rounds; the GL painter's path was checked by hand in the desktop sandbox (a `Loader` turned on and off eleven times: no fault, the area empty after, 0 ticks idle), egui-wgpu's is left to its `retain`. Neither has an automated test.
- **MSAA and the blit:** resolving before the blit keeps composition unchanged. The cost is to be measured at M3.
- **The web wrapper's owner** is ljos-web. Its embedding API has to agree with `SurfaceView`.
- **The web after M3.** The harness checks one flat-shaded cube with no MSAA, which is why its pictures match exactly. MSAA (M3) on WebGL2 needs a multisampled renderbuffer and resolve, which wgpu's GL backend does but with WebGL2's own sample counts; textures (M4) and HDR need formats WebGL2 may lack (`Rgba16Float` render targets are an extension there). Each milestone's scenes go through the harness on both backends before M8 is closed.
