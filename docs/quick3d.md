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
  shell's wgpu painter                         GL painter, R, snapshots, tests  web <canvas>, own windows
               └──────────────── all draw with ljos_3d::Renderer ───────────────────────┘
```

| Crate | Owns | Interface | Depends on |
|---|---|---|---|
| `ljos_3d` | the scene (nodes, transforms, meshes, materials, cameras, lights, environment, revision), the renderer, its shaders and GPU caches | `Scene` (`add`, `remove`, `node_mut`, `set_camera`, `set_environment`, `revision`, `walk`); with `gpu`: `Renderer::{new, render, prepare_blit, blit}`, `Target`, `Offscreen`, `SurfaceView` | slotmap, thiserror; wgpu with `gpu` |
| `ljos_quick3d` (M2) | the QML types of `QtQuick3D`, their properties and defaults, the item that is `View3D` | `register(&mut Registry)`, like `ljos_lottie` | `ljos` facade, `ljos_3d` without `gpu` |
| `ljos_egui`, feature `quick3d` | the egui-wgpu adapter | `Scene3dResources::{install, retain}`, `Scene3dPaint::shape(rect)` | egui-wgpu, `ljos_3d` |
| `ljos_shell` | turning `quick3d` on and choosing the painter | none of its own | `ljos_egui[quick3d]` |
| ljos-web | the wasm-bindgen wrapper and the canvas | its own, over `SurfaceView` | `ljos_3d` |

**Which crates may touch wgpu:** `ljos_3d` (feature `gpu`), `ljos_egui` (feature `quick3d`) and `ljos_shell` (ARCHITECTURE.md §4, crate rules). `ljos_render`'s B1 backend joins them when it is built. The QML types never see a device: they write the scene, so they run and test with no GPU.

**The IR/runtime seam.** QtQuick3D is a module of native types like any other (ARCHITECTURE.md §5.5): the compiler sees them through `ljos.txt`, and the engine runs them from IR. The scene is the 3D side's retained tree. Each 3D object owns a `NodeId` in the `Scene` of its `View3D` (or of the `View3D` whose `importScene` names it), and a property write is a write to that node, which bumps the revision. `View3D` holds the scene in an `Arc` and paints a `DrawOp` with it. On a change it makes a new `Arc` (copy on write, `Arc::make_mut`), so a frame in flight keeps the scene it was given.

**The paint seam.** `ljos_egui::scene3d` uses egui-wgpu's paint callback:
- `prepare` renders the scene into the view's `Target`, on egui's device and egui's encoder, before egui's pass begins;
- `paint` blits the target into egui's pass, where egui has set the viewport and scissor to the item.

So the 3D picture lands in paint order: items declared after the `View3D` are drawn over it and items before it under it, with no extra pass and no copy back to the CPU. A target is kept per view key and dropped by `retain` when the view goes.

**The off-screen seam.** The GL painter, R (vello_cpu), the MCP snapshot and the BDD pictures have no wgpu pass. For them `ljos_3d::Offscreen` renders on a device of its own and reads the picture back, and the view paints it as an image. The readback happens only when the revision or size changed, so idle stays free. A window that shows a `View3D` prefers the wgpu painter:
- the shell picks wgpu at startup when the application's IR imports `QtQuick3D`;
- otherwise a window switches painter once, the first time a `View3D` is shown in it, as a window drawn on the processor switches to GL today.

**The web seam.** `ljos_3d::SurfaceView::new(target, w, h)` makes its own instance (WebGPU if the browser has it, else WebGL2), device and surface; `resize`; `draw(&Scene) -> bool` presents only on a new revision. On the web the target is `wgpu::SurfaceTarget::Canvas`. ljos-web owns the wasm-bindgen wrapper; `ljos_3d` takes no wasm-bindgen dependency of its own.

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

**Startup risk.** The shell paints with GL by default, so a `View3D` window brings up a wgpu (Vulkan) device as well. The NVIDIA ICD already costs startup time on machines without NVIDIA hardware (the shell leaves it out). The first bench run measures device creation on its own, against `qml6`'s RHI startup. If wgpu loses there, the fallback is wgpu on the GL painter's own EGL context (wgpu's GLES backend on an external context), still one device per process.

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
| **M8 Web** | ljos-web draws through `SurfaceView` into a `<canvas>`; the WebGL2 fallback checked | the same scenes render in a browser with WebGPU and with WebGL2 |
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
- **Painter choice.** wgpu for windows that show a `View3D`. Readback for GL, R, snapshots and tests.
- **Own small maths** instead of glam: a few dozen lines, no dependency, column-major and matching WGSL.
- **wgpu 30.0.1,** the version egui-wgpu 0.37 is built on, with backends chosen per target.

## Open risks

- **Startup with the GL default painter** (§ Performance): a second device per process, with Vulkan initialisation and the NVIDIA ICD.
- **No oracle yet:** `qt6-quick3d` is not installed. Conventions taken from the documentation are not checked against qml6:
  - which way lights point;
  - the order euler rotations apply in (the spike applies z, then x, then y);
  - the default tonemapping;
  - the strength of ambient light.
- **Target lifetime:** a view that is destroyed must release its target. `retain` exists; the engine does not call it yet.
- **MSAA and the blit:** resolving before the blit keeps composition unchanged. The cost is to be measured at M3.
- **The web wrapper's owner** is ljos-web. Its embedding API has to agree with `SurfaceView`.
