# 02 — Rendering architecture: a retained QML scene on top of egui

Research date: 2026-10-10. Versions are from crates.io and upstream `main` branches as of that date. Clean-room: Qt and Slint were studied only through their public documentation. egui, vello, parley, xilem/masonry, accesskit and winit source was read (all MIT/Apache).

---

## 1. Summary of key findings

- **egui is at 0.36.2** (2026-09-08; MSRV 1.95, wgpu 30, winit 0.30.13, accesskit 0.25). The unreleased 0.37 adds system-font fallback through `fontique`, colour emoji, `egui::Role = accesskit::Role` and a deprecation of `Id::new`. The API changes every ~6–10 weeks. Plan to pin it and upgrade deliberately.
- **egui does not retain anything that would help a scene graph.** Every frame it rebuilds all `Shape`s and re-tessellates them, then `egui-wgpu` re-uploads every vertex and index. `Shape::Mesh(Arc<Mesh>)` skips tessellation but is still memcpy'd (`append_ref`) and re-uploaded. Its AccessKit output is a **full `TreeUpdate` every frame**. There is no damage tracking or partial present. Being "reactive" only means egui runs no frames while idle, which already gives zero idle CPU.
- **egui's text engine is not good enough for QML `Text`/`TextInput` in a localisable app.** Shaping is now harfrust (0.35) and rasterisation is skrifa + vello_cpu (0.34), but there is **no bidi or RTL** (the source says "would need script-aware splitting once RTL/bidi support is added"). Runs are segmented by font face, not script. A Parley migration has been talked about upstream but not started.
- **Parley 0.12.0 (2026-10-09) is the right text engine.** It has bidi, ICU4X segmentation, grapheme-cluster carets, CSS white-space and vertical-align, fontique fallback, a `PlainEditor` with IME support, and an `accesskit` feature. It pins the **same skrifa 0.44 / harfrust 0.12 / read-fonts 0.41 as epaint**, so the font stack is mostly shared, not duplicated.
- **Qt Quick `opacity` is *not* group opacity.** Per the Qt docs, "the specified opacity is also applied individually to child items". Only `layer.enabled` renders the item to an offscreen texture and applies opacity to that texture. So plain opacity is just a multiplied vertex alpha (which matches egui's `Painter::multiply_opacity`). Offscreen compositing is only needed for `layer.*`, effects, `ShaderEffectSource`, and rotated clips.
- **epaint geometry is more flexible than it looks.** `RectShape` has `angle` (0.34+), `blur_width` and `brush`, and `TextShape` has `angle`. A `Mesh` is just 20-byte vertices plus u32 indices, so any affine transform can be applied to cached vertices on the CPU. Layer transforms (`TSTransform`) only support translate and uniform scale. Clipping is one axis-aligned rect per `ClippedShape`. epaint has no gradients except per-vertex colour, no stencil, and no offscreen layers.
- **`egui_wgpu::CallbackTrait` is the escape hatch for custom rendering.** `prepare()` gets the device, queue and egui's `CommandEncoder`, so offscreen passes can be encoded there. `paint()` then draws inside egui's own render pass, in correct z-order with egui shapes. `Renderer::register_native_texture` turns any `wgpu::Texture` into an egui `TextureId`.
- **Vello was reorganised on 2026-10-02.** `vello_hybrid` is now **`vello_gpu` 0.3.0** (sparse strips: CPU preprocessing, GPU raster with vertex/fragment shaders only, wgpu 30 and WebGL2 backends, so no compute needed and it is mobile-friendly). **`vello_cpu` 0.3.0** is described as "currently overall more mature". The compute `vello` 0.11 is now under `research/`. The sparse-strip renderers have **no scene caching**: every `fill_path` is re-flattened and re-stripped. Anything cached therefore has to be cached as a texture.
- **Masonry is the closest prior art for "retained tree, immediate-style paint".** Its passes are event → rewrite (mutate, update_*, layout, compose) → paint and accessibility. It caches one per-widget scene triple `(pre, main, post)` in local coordinates and re-appends it with a transform. Transform-only changes go through a cheap `compose` pass instead of layout. It moved to a `masonry_imaging` abstraction with vello, vello_cpu, vello_hybrid and Skia backends, which supports our plan to make an owned display list the backend contract. Masonry and Xilem have not been released since 0.4.0 (2025-10-29).
- **Qt's documented scene-graph strategy** keeps retained GPU geometry per "batch root", so scrolling updates one matrix. It sorts opaque content front-to-back with a depth buffer, uses scissor for axis-aligned clips and stencil for rotated clips, and targets fewer than 10 batches per frame. It runs a **threaded render loop on mobile**, where the GUI thread is blocked only during a short "sync" phase.
- **Android is the weak spot, and it sits below egui.** winit's Android backend only does `show_soft_input`/`hide_soft_input` for IME: no composition, no surrounding text, `set_ime_purpose` is a no-op, and 0.31-beta removes `MonitorHandle` (so no refresh rate) on Android. egui 0.36's "mobile keyboard" improvements are for **eframe web only**. AccessKit now has Android and iOS adapters, and accesskit_winit on Android requires GameActivity.
- **AccessKit 0.25 has graft nodes.** `Node::set_tree_id(TreeId)` mounts an independently updated subtree. This lets Ljós push **incremental** accessibility updates for its retained tree under egui's per-frame root.
- **Testing:** egui_kittest 0.36.2 drives frames through `Harness::step/run`, queries the AccessKit tree through `kittest` 0.5, and snapshots through a pluggable `TestRenderer` (wgpu by default; YIQ threshold 0.6, `max_failed_pixels`). For deterministic comparison against Qt's pictures, a **vello_cpu reference backend** needs no GPU, unlike wgpu's software adapters (lavapipe/llvmpipe/WARP), which vary by driver.

**Recommendation in one line:** use option **(d) hybrid**, delivered in phases. Ljós owns a retained render tree and a backend-neutral **display list per node** (Masonry-style), with its own layout, animation, hit testing, focus and text (Parley). egui is the **host**: windowing via egui-winit, input/IME/clipboard, the frame loop, embedding egui widgets in QML and QML in egui. Painting has two backends behind one trait: **B0 `epaint`** (cached `Arc<Mesh>` per node, ships first, best for kittest and embedding) and **B1 `ljos-wgpu`** (retained instanced SDF quads, glyph and image atlases, offscreen layers, vello_gpu for paths, injected via `egui_wgpu::Callback`). A third, **`vello_cpu`**, is the deterministic reference renderer for pixel tests against Qt.

---

## 2. Detailed findings

### 2.1 egui internals relevant to Ljós (0.36.2, `main` @ 2026-10-09)

**Frame lifecycle.**
- `Context::run_ui(RawInput, |ui| …) -> FullOutput` replaced `Context::run` in 0.34, and `Ui` now derefs to `Context`. `FullOutput` contains `platform_output` (cursor, IME, clipboard, `accesskit_update`), `textures_delta`, `shapes: Vec<ClippedShape>`, `pixels_per_point` and `viewport_output` (repaint delay per viewport).
- The integration calls `ctx.tessellate(shapes, ppp) -> Vec<ClippedPrimitive>`, then `egui_wgpu::Renderer::update_texture`, `update_buffers` (which writes **all** vertices and indices every frame) and `render`.
- egui supports multi-pass: `request_discard` / `will_discard`, with `max_passes` 2 used by egui_taffy for measure-then-layout. Each discarded pass re-runs the whole UI closure.

**Repaint scheduling.**
- `request_repaint()`, `request_repaint_after(Duration)`, `request_repaint_of(ViewportId)` and `set_request_repaint_callback`.
- `Context` is `Clone + Send + Sync`, so tokio tasks can wake the UI.
- eframe and egui-winit are reactive: no frames run without input or a request. **Most winit events, including cursor moves, trigger a full frame**, which means a full re-tessellation and upload on every hover.
- `ViewportInfo::occluded`/`visible` were added in 0.34 and can stop the frame clock.

**Painter and shapes.** `Shape` variants are:
- `Noop`, `Vec`, `Circle`, `Ellipse`, `LineSegment`, `Path`, `Band` (new), `Rect`, `Text`, `Mesh(Arc<Mesh>)`, `QuadraticBezier`, `CubicBezier` and `Callback(PaintCallback)`.
- `RectShape` has `corner_radius: CornerRadius` (u8 per corner), fill, stroke, `stroke_kind`, `blur_width`, `brush: Option<Arc<Brush>>` (texture fill) and `angle` (with pivot).
- `TextShape` has `galley`, `angle`, `opacity_factor` and `override_text_color`.
- `Mesh::rotate(Rot2, origin)` exists. A `Vertex` is `{pos: Pos2, uv: Pos2, color: Color32}`, premultiplied sRGB, 20 bytes.
- Each `Mesh` has **one** `TextureId`. egui merges consecutive primitives only when texture and clip rect match, so mixing two atlases breaks batching.
- `Painter::with_clip_rect`, `set_opacity`, `multiply_opacity` and `set_invisible` exist.
- **Clips are axis-aligned rects only.** There are **no gradients** (only per-vertex colour) and **no stencil, masks or offscreen layers**.
- AA is "feathering": a ~1 px alpha ramp produced by the tessellator, with no MSAA. If a mesh is tessellated at scale 1 and then scaled, the feather scales with it.

**Layers.**
- `LayerId { order: Order, id }` with `Order ∈ {Background, Middle, Foreground, Tooltip, Debug}`. `Area`s create layers, and z within an `Order` is managed by `Memory::areas`.
- `Context::set_transform_layer(LayerId, TSTransform)` only supports **translation and uniform scale**. `transform_layer_shapes` applies a transform to shapes after the fact.
- There is no per-layer render target.

**Ids, memory, interaction.**
- `Id` hashing is being reworked: 0.37 deprecates `Id::new` in favour of `Id::unique`, `IdSalt*` and `AsIdSalt`.
- `Ui::interact(rect, id, Sense) -> Response` hit-tests **axis-aligned rects** against the previous pass's widget rects. 0.34 added "allow moving existing widgets to the top of interaction stack".
- Focus lives in `Memory` (`memory.focused()`, `request_focus`).
- IME arrives as `Event::Ime(ImeEvent)`. The app reports the caret through `platform_output.ime` (rect and cursor rect). `Memory::owns_ime_events` was added in 0.35.

**Text.**
- `LayoutJob` → `Galley`, cached in a galley cache that evicts entries unused for a frame.
- Shaping uses **harfrust** (0.35). Rasterisation moved to **skrifa + vello_cpu** (0.34), adding hinting, variable fonts and `subpixel_binning`.
- 0.37 (unreleased) adds `GlyphRasterizer` and `FontProvider` (fontique system fonts), which bring colour emoji and arbitrary scripts.
- There is **no bidi**: `epaint/src/text/unicode.rs` has `TODO(emilk): heed bidi characters`, and `text_layout.rs` has "need script-aware splitting once RTL/bidi support is added".
- There is no justification, no per-script itemisation and no rich-text markup.

**Accessibility.**
- AccessKit is always on (since 0.34). Widgets add nodes through `ctx.accesskit_node_builder(id, |node| …)`, with parents taken from the `Ui` stack.
- `end_pass` collects **all** nodes into one `TreeUpdate { nodes, tree: Some(TreeInfo), tree_id: TreeId::ROOT, focus }` every frame.
- 0.37 hides invisible widgets from the tree and adds `Area::role` and `Response::accessible_name`.

**Custom rendering.**
- `PaintCallback { rect, callback: Arc<dyn Any> }`. For wgpu, `egui_wgpu::Callback::new_paint_callback(rect, impl CallbackTrait)` is used.
- `CallbackTrait` has `prepare(device, queue, screen_descriptor, egui_encoder, resources) -> Vec<CommandBuffer>`, then `finish_prepare`, then `paint(info, &mut RenderPass<'static>, resources)`.
- `prepare` can record **offscreen render passes**. `paint` draws inside egui's pass with the viewport and scissor set to the callback rect.
- `Renderer::register_native_texture(device, &TextureView, FilterMode) -> TextureId` and `update_egui_texture_from_wgpu_texture` expose our textures to epaint meshes.
- `TextureHandle::set_partial(pos, image, options)` supports incremental atlas uploads through egui's texture manager.

**eframe and platforms.**
- eframe 0.36.2 targets native wgpu or glow, and web (`egui_web` is new on main).
- **Android**: `android-game-activity` / `android-native-activity` features and an `examples/hello_android` built with cargo-apk (NDK 29). **AccessKit on Android requires GameActivity** (eframe has a compile error otherwise).
- `egui_inspection` and `egui_mcp` (0.35) expose the AccessKit tree and input injection on port 5719. This is useful later for driving Ljós apps from agents.

**egui_kittest 0.36.2.**
- `Harness::new_ui(|ui| …)` or `new_ui_state`/`new_eframe`, with `step()`, `run()` (until no repaint requested, up to `max_steps`) and `run_steps(n)`.
- Queries (`get_by_label`, `get_by_role`, …) come from `kittest` 0.5, which consumes the AccessKit tree.
- `render() -> RgbaImage` and `snapshot(name)` go through `TestRenderer { handle_delta, render(ctx, &FullOutput) }`. The default is wgpu and executes paint callbacks.
- `kittest.toml` sets `threshold = 0.6` (YIQ), `max_failed_pixels = 0` and per-OS overrides.
- `Harness::builder().renderer(impl TestRenderer)` allows a **custom renderer**, which is the hook for a vello_cpu-based Ljós snapshot renderer.

### 2.2 Retained/declarative layers people have built on egui

- **egui_taffy 0.14** (egui 0.36, taffy 0.9.2): keeps `TaffyState` keyed by `egui::Id` across frames. It needs multipass (`max_passes = 2`) and `request_discard` when sizes change, and has known text-wrapping issues inside flex.
- **egui_flex 0.8**: a lighter flex layout in the same "immediate API, cached measurements" style.
- **egui_mobius 0.6** (egui 0.36): signal/slot, a `Dispatcher` and `Dynamic<T>` reactive state around docked "citizen" panels. It is about state and message passing and does nothing for rendering.

All three keep egui's model of re-emitting widgets every frame and caching measurements by `Id`. **None of them retains geometry, so none solves the cost model that a QML scene graph needs.** That rules out using any of them as our foundation.

### 2.3 Qt Quick semantics that constrain the renderer (from doc.qt.io only)

- **Paint order**: siblings are ordered by `z`, then by declaration order. Children paint above the parent's content. Children with negative `z` paint **under the parent's content**.
- **Opacity** is applied individually to children (not grouped). `opacity` does not affect input; `visible: false` propagates to children and stops mouse input but not key input.
- **`layer.enabled`**: "rendered into an offscreen surface and cached until it is changed". Opacity is applied to the texture, and memory is width × height × 4. `layer.smooth`, `layer.textureSize`, `layer.effect` (any Item, usually ShaderEffect).
- **Clip**: an item clips itself and its children to its bounding rect. Qt uses scissor for axis-aligned clips and **stencil for clips at non-90° rotation**. Clip limits batching, and the docs say clip in delegates "should be avoided at all costs".
- **Transforms**: `rotation` and `scale` apply around `transformOrigin` (default `Item.Center`). The `transform` list (Rotation, Scale, Translate, Matrix4x4) is applied before those. Negative scale mirrors.
- **Default renderer**:
  - Retained VBOs per batch root, so scrolling only updates the root matrix.
  - An opaque pass (reordered freely, depth-tested, no blending, which the docs call "costly on mobile") and an alpha pass.
  - A target of fewer than 10 batches with 3–4 of them opaque.
  - Large or mipmapped images are kept out of the atlas.
  - Debug variables `QSG_VISUALIZE=batches|clip|changes|overdraw` and `QSG_RENDER_TIMING`. Ljós should offer equivalent switches.
- **Render loops**:
  - **threaded** is the default on mobile, Vulkan and Metal. The render thread blocks the GUI thread during **sync**; `updatePolish` runs there, then `updatePaintNode` for changed items. "This is the only time the QML items and the nodes in the scene graph interact."
  - Animations are driven by the vsync-paced render loop. A GUI-thread driver runs property animations and a render-thread driver runs `Animator`s.
  - The loop falls back to timers when no window is renderable or vsync is unreliable.
- **Performance guidance**:
  - Lazy creation through `Loader.active` and `asynchronous`.
  - `ListView.cacheBuffer` for off-screen delegates.
  - `Image.asynchronous` (decoded on a low-priority worker) and `sourceSize` (decode at display size).
  - "An image with one translucent pixel is treated as fully translucent."

**Implications for Ljós:**
- Plain opacity needs no offscreen pass.
- Offscreen passes are required for `layer.*`, `ShaderEffectSource`, `OpacityMask`/`MultiEffect`, and rotated clips (with the stencil alternative).
- Hit testing and clips must handle full 2D affine transforms (QML `Rotation` with `axis` is 3D, so it needs a perspective `Matrix4x4`).

### 2.4 Options evaluated

| Criterion | (a) QML → egui widgets every frame | (b) Retained tree, cached epaint meshes | (c) Retained tree, own wgpu/vello renderer; egui as host | (d) Hybrid: (b) now, (c) behind same display list |
|---|---|---|---|---|
| 1k–10k items | O(n) rebuild of widgets, Ids, layout and tessellation per frame, plus a full JS-side traversal. Breaks down past ~1–2k | O(visible) Arc pushes plus memcpy plus full vertex upload (~50 verts × 20 B per rounded rect ≈ 1 KB/item, so 5k items ≈ 5 MB/frame) | O(changed) uploads. Static content costs only draw calls; scrolling only updates a matrix | Starts as (b); hot paths move to (c) |
| Render on demand, idle | egui reactive (OK) | OK | OK (egui still repaints the full frame) | OK |
| Damage tracking | none | none (egui has no partial present) | possible per layer, but egui-wgpu clears the frame anyway | layer caching only |
| Clipping | axis-aligned only | axis-aligned; rotated clip → offscreen | scissor, stencil or masks | rotated clips through B1 |
| `layer.enabled`, effects | impossible | needs an offscreen hook (callback) | native | B1 |
| Rotation, scale | `TSTransform` has no rotation; widgets are axis-aligned | transform cached vertices (any affine); text resampled | native | yes |
| Rounded rect, border, gradient, shadow | egui styling only | rounded rect native; gradient via vertex colours (approximate); shadow via `blur_width` | SDF shader: exact, 4 verts, 1 instance | B0 approximate, B1 exact |
| Text quality and i18n | egui text (no bidi) | Parley glyph quads into our atlas | same | same |
| Mobile GPU | fine | heavy uploads at 120 Hz | best (retained, few passes, no MSAA) | best eventually |
| Accessibility | egui widgets (free but wrong semantics) | Ljós-built nodes | same | same |
| kittest | native | native (shapes in `FullOutput`) | works (wgpu renderer runs callbacks) but snapshots need wgpu | B0 for unit tests, B1 for snapshots |
| Embed egui inside QML | trivial | natural (same painter, same order) | segment the Ljós draw around the egui child | yes |

Option (a) is rejected: QML semantics such as anchors, z, transforms, layers, `Behavior` and per-item animation do not map onto egui's layout cursor, and its costs scale with total items, not changed items. Option (c) alone delays a usable product and needs a whole renderer before the first pixel. **Option (d)** gets the API and semantics right early on B0. A **display-list contract** keeps the backends interchangeable, which is the approach Masonry took with `masonry_imaging`.

### 2.5 Relevant crates (versions from crates.io, 2026-10-10)

| Crate | Version (date) | Role for Ljós | Notes |
|---|---|---|---|
| egui / epaint / egui-wgpu / egui-winit / eframe / egui_kittest | 0.36.2 (2026-09-08) | host, B0, tests | MSRV 1.95; wgpu 30; winit 0.30.13 |
| wgpu | 30.0.1 (2026-08-22) | GPU | same major as egui-wgpu 0.36 and vello_gpu 0.3 |
| vello_gpu (ex-vello_hybrid) | 0.3.0 (2026-10-02) | paths, Shapes, Canvas → textures | no compute; wgpu + WebGL2; `push_layer`, `push_clip_rect/path`, filters; no scene caching |
| vello_cpu / vello_common | 0.3.0 (2026-10-02) | reference renderer, CPU fallback | most mature; SIMD + MT; already a transitive dependency of epaint (0.2) |
| vello (compute) | 0.11.0 (2026-10-02) | not used | research-grade; needs compute shaders |
| parley / fontique | 0.12.0 (2026-10-09) | text layout, editing, fonts | bidi, ICU4X, `PlainEditor`, new `parley_engine` |
| harfrust / skrifa / swash | 0.14 / 0.48 / 0.2.10 | shaping, glyph outlines | parley and epaint both pin harfrust 0.12, skrifa 0.44 |
| cosmic-text | 0.19.0 (2026-04) | rejected | own shaping and layout; would duplicate fontique |
| taffy | 0.14.0 (2026-08-24) | optional only | flex/grid ≠ QtQuick.Layouts semantics |
| kurbo / peniko | 0.13.1 / 0.6.1 | geometry, brushes | `Affine`, `BezPath`, `Rect`, `RoundedRect`, `Gradient` |
| accesskit / accesskit_winit | 0.25.1 / 0.34.1 | accessibility | graft subtrees (`set_tree_id`); Android and iOS adapters |
| tiny-skia | 0.12.0 | rejected for reference | vello_cpu is more complete and closer to our GPU model |
| resvg | 0.48.1 | SVG `Image` sources | rasterise at `sourceSize` |
| masonry / xilem | 0.4.0 (2025-10-29) | architecture reference | main on vello 0.8, wgpu 28 |
| iced | 0.14.0 | reference | wgpu quad pipeline (instanced SDF quads), tiny-skia fallback |
| blitz-dom / anyrender(_vello) | 0.2.4 / 0.14–0.15 | reference | Stylo + Taffy + Parley; AnyRender 2D abstraction; beta |
| freya | 0.4.3 (0.5 rc) | reference | Skia; own reactive core; Android support |
| makepad | 1.0.0 | reference | instanced, shader-per-widget drawing; cached draw lists |
| egui_taffy / egui_flex / egui_mobius | 0.14 / 0.8 / 0.6 | not foundations | see §2.2 |

**Lessons drawn from these:**
- **Masonry**:
  - Use pass flags rather than full rebuilds.
  - Keep a cheap `compose` pass for transform-only changes (scroll, animate x/y).
  - Cache per-widget scenes in local space and append them with a transform.
  - A `stashed` state removes subtrees without destroying them, which maps onto `visible: false` and off-screen delegates.
  - Accessibility is built as a render pass from the same tree.
- **Qt** (docs): batch roots and retained VBOs, the opaque/alpha split, scissor-then-stencil clipping, the threaded loop with a short sync, and render-thread-only Animators.
- **Iced, GPUI (Zed's "120 FPS" blog) and Makepad** all draw UI primitives as **instanced quads evaluated by SDF in the fragment shader**: rounded rects, borders, shadows and gradients, with analytic AA and about 64–96 B per instance. That is 10–20× less vertex traffic than tessellated meshes and keeps AA exact under scale.
- **Slint** (docs): the software renderer has partial rendering but no rotation or scale and "western scripts" only. Its Vello renderer "requires a GPU that supports compute shaders" and is never auto-selected. Avoiding a compute-only renderer on mobile is the right call, and a CPU renderer is valuable for constrained devices and tests.
- **Vello sparse strips**: CPU cost is proportional to path complexity *every* frame, so cache path content as textures, not as scenes.

### 2.6 Text: why Parley, and how glyphs reach the GPU

QML `Text` needs:
- `wrapMode`
- `elide` (including middle)
- `horizontalAlignment` including `Justify`, with RTL auto-mirroring through `LayoutMirroring`
- `lineHeight` / `lineHeightMode`
- `maximumLineCount`
- `font.letterSpacing` / `wordSpacing` / `capitalization` / `hintingPreference`
- `textFormat` (PlainText, StyledText, RichText, MarkdownText)
- `fontSizeMode` (Fit)
- `TextInput` and `TextEdit` with selection, IME preedit and password echo

Localisation requires bidi, and HAL-C2's chat content will mix scripts and emoji.

Parley 0.12 covers layout and editing:
- `PlainEditor` and `PlainEditorDriver` handle selection, IME compose and cursor movement.
- Clusters are grapheme-correct.
- Inline boxes support `StyledText` images.

Glyph rasterisation:
- Rasterise with skrifa through **vello_cpu** (as epaint does) or swash.
- Pack into a **Ljós glyph atlas** that also holds a 1×1 white texel, so solid geometry and glyphs share one texture and egui can merge draws.
- In B0 the atlas is an egui texture (`TextureHandle::set_partial`). In B1 it is our own `wgpu::Texture`.
- Colour glyphs (COLR/CBDT) go into an RGBA region.
- Rotated or scaled text is resampled from the atlas, the same compromise as egui's `TextShape::angle`. When the scale is stable (e.g. `scale: 2` held for more than N frames), re-rasterise at the effective size.

---

## 3. Recommendations for Ljós

### 3.1 Decisions

1. **The retained render tree is owned by Ljós. egui never sees QML items as widgets.** QML objects (QObject-like, JS-bound) own properties. A separate `RenderTree` holds only render-relevant state and is updated during a **sync** step, as in Qt's sync phase. This keeps JS and bindings away from GPU state and makes a later render thread possible.
2. **Each node caches a display list (`Arc<[DrawOp]>`) in local coordinates.** It is rebuilt only when that node's content is dirty. Transform, opacity, clip and z changes never rebuild display lists; they only change composition.
3. **Three backends consume display lists:**
   - **B0 `EpaintBackend`** (M0–M2): per-node cached `Arc<Mesh>`, re-transformed on transform change. Text uses our atlas. Gradients use vertex colours on sub-divided meshes. Shadows use `RectShape::blur_width` or a precomputed blurred nine-patch. Axis-aligned clips use `Painter::with_clip_rect`. `layer.enabled` and rotated clips fall back to approximations: opacity applied per child, and the clip rect is the axis-aligned bounding box (with a debug warning).
   - **B1 `WgpuBackend`** (M3+), injected as one `egui_wgpu::Callback` per contiguous Ljós segment:
     - Instanced SDF quads for rect, rounded rect, border, gradient and shadow.
     - Textured quads for glyphs and images.
     - Retained instance buffers per **batch root**, so a subtree whose only change is its transform updates one uniform.
     - Offscreen targets for `layer.*`, effects and rotated clips, rendered in `prepare()`.
     - Stencil-free masks via an alpha-mask texture.
     - **vello_gpu** renders `Shape`/`Canvas`/SVG paths into cached atlas textures.
   - **R `VelloCpuBackend`**: deterministic reference renderer for tests, and a fallback on devices without a usable GPU.
4. **Text uses Parley 0.12 plus fontique and our own glyph atlas.** Do not use egui's `Galley` for QML text. egui's own fonts can be trimmed to a small set (`default_fonts` stops being a default in 0.37).
5. **Layout is our own implementation of QML semantics:**
   - Anchors are resolved eagerly as geometry bindings.
   - Positioners (Row, Column, Grid, Flow) and QtQuick.Layouts (`Layout.fillWidth`, preferred/min/max, `Layout.alignment`, spans) run in a deferred **polish** pass.
   - Taffy is **not** the core engine. Its flex and grid semantics differ: implicit sizes, `fillWidth` stretch distribution, and baseline and spacing rules. It can be offered as an optional `Ljos.Flex` item.
6. **Input, focus and hit testing are our own.** egui `RawInput` events are routed through a Ljós `InputRouter`:
   - Hit-test in reverse paint order with inverse world transforms, ancestor clips and `containmentMask`.
   - QML focus (`activeFocus`, `FocusScope`, `KeyNavigation`).
   - Gesture handlers (TapHandler, DragHandler, Flickable kinetics).
   - egui's `ScrollArea` and `Sense` are not used for QML content.
   - **Bridge rule for embedded egui widgets:** a `EguiItem` gets a child `Ui` at its world AABB (only translation and uniform scale are allowed; rotation is unsupported). Any QML item painted *above* an `EguiItem` and accepting input registers an "occluder" `ui.interact(rect, id, Sense::click_and_drag())` *after* the embed, so egui's hit-test respects QML stacking.
   - When a Ljós `TextInput` has focus, write `platform_output.ime` (caret rect) and consume `Event::Ime`.
7. **Accessibility is generated from the retained tree and diffed incrementally:**
   - When running in our own shell, push `TreeUpdate`s through our own `accesskit_winit::Adapter` as a **grafted subtree** (`TreeId` UUIDv4, graft node with `set_tree_id`), containing only nodes whose `A11Y` flag is dirty.
   - When embedded in a third-party eframe app, fall back to emitting nodes through `ctx.accesskit_node_builder` each frame.
   - Off-screen but instantiated items stay in the tree, as on the web. Culled-for-paint ≠ hidden-for-a11y.
8. **Host shell:** M0 runs on **eframe** (fastest spike, gets Android and web). From M2, Ljós uses its own **`ljos_shell`** on egui-winit + egui-wgpu. It owns the event loop, AccessKit adapter, surface lifecycle (Android `Suspended` drops the surface and evicts GPU caches), present mode, an optional render thread, and hot-reload hooks. `ljos_egui::QmlView` remains an egui widget so QML can be embedded in any egui app.
9. **Zero idle CPU.** No timers run unless an animation, a `Timer`, or `request_repaint_after` asks for them. Property writes mark dirty, and the first dirty mark of a frame calls `ctx.request_repaint()`. When the viewport is occluded or the app is paused, the frame clock stops: animations jump to the wall-clock position on resume, matching Qt's timer fallback semantics.
10. **Off-screen content does not cost resources:**
    - Display lists are built lazily, on first intersection with the viewport (plus margin).
    - GPU resources (layer textures, images, path textures) are kept in an LRU with byte budgets (e.g. 64 MB desktop, 24 MB mobile by default), driven by `last_visible_frame`.
    - Image decoding is async at `sourceSize` on a bounded worker pool (tokio `spawn_blocking` with a semaphore), with uploads budgeted per frame (≈4 MB per frame on mobile).
    - Delegate virtualisation (`ListView`, `Repeater` in Flickable) lives in the item layer, with a `cacheBuffer`-like margin.
11. **Colour and AA match Qt for comparisons.** Use premultiplied sRGB with blending in gamma space (egui's model, and Qt's default). Rectangle AA should follow Qt's documented defaults. Use 1 QML logical pixel = 1 egui point.

### 3.2 Architecture

```
 QML/JS world (rquickjs, single "QML thread")                     Ljós render side
 ┌─────────────────────────────────────────────┐   sync (diff)   ┌─────────────────────────────────────┐
 │ QObject tree  ── properties / bindings ──┐   │ ─────────────▶ │ RenderTree (slotmap of RenderNode)  │
 │ Items: Rectangle, Text, Image, Flickable,│   │  ChangeSet     │  transform, opacity, clip, z-order, │
 │ ListView(delegates), Loader, EguiItem    │   │                │  DirtyFlags, Arc<DisplayList>,      │
 │ Animations (frame clock)  Timers         │   │                │  subtree AABB, LayerSpec, a11y      │
 │ polish: Layouts/positioners/Text(Parley) │   │                └───────┬──────────────┬──────────────┘
 └──────────────▲───────────────────────────┘   │                        │ cull+order   │ diff
                │ input/focus/IME (InputRouter: hit test on RenderTree)  ▼              ▼
 ┌──────────────┴──────────────────────────────────────────┐   ┌─────────────────┐ ┌──────────────────┐
 │ egui host (egui-winit + Context::run_ui)                 │   │ RenderBackend   │ │ A11y: TreeUpdate │
 │  RawInput ─▶ events ─▶ InputRouter                       │   │  B0 epaint      │ │ graft subtree →  │
 │  QmlView::ui(&mut Ui): allocate rect, occluders,         │◀──│  B1 wgpu+vello  │ │ accesskit adapter│
 │    embedded egui widgets (EguiItem → child Ui)           │   │  R  vello_cpu   │ └──────────────────┘
 │  FullOutput: shapes(+Callback) / textures / ime / a11y   │   └────────┬────────┘
 └──────────────┬──────────────────────────────────────────┘            │ prepare(): layers, atlases,
                ▼                                                       │ vello_gpu paths → textures
 egui-wgpu Renderer: update_buffers → one render pass: egui meshes ⟷ Ljós Callback segments → present
```

The per-frame pipeline runs on the QML thread in v1:

```
1 input      : egui RawInput → InputRouter (hit test, focus, gestures) → JS handlers
2 settle     : bindings/signals drain (bounded work budget; overflow → next frame, request_repaint)
3 animate    : FrameClock.tick(predicted present time) → animations write properties
4 polish     : Layouts, positioners, Text layout (Parley), implicit sizes; loop ≤ 8 iterations
5 sync       : dirty items → RenderNode fields; rebuild DisplayList only for CONTENT-dirty
6 compose    : world transforms, subtree AABBs, paint order (z, decl index, negative z), culling
7 a11y diff  : TreeUpdate for A11Y-dirty nodes
8 encode     : backend.encode(FrameView) → egui shapes / callback; textures via egui or own wgpu
9 egui end   : FullOutput → tessellate (B0) → egui-wgpu render → present (Fifo vsync)
```

**Threading.** v1 is single-threaded, which is simplest and works the same on Android and iOS. Steps 5–6 produce an immutable `FrameSnapshot` (Arc'd display lists, transforms, order). This is the seam for v2: a **render thread** that runs step 9 and the B1 `prepare`. The QML thread is blocked only while the snapshot is swapped, mirroring Qt's sync. Render-thread "animators" for transform and opacity can come later. rquickjs runtimes are not `Send` by default, so JS stays on one thread. tokio runs networking on its own pool and wakes the UI via `Context::request_repaint`.

**Frame pacing.**
- Use `PresentMode::Fifo` (AutoVsync).
- Animations sample at the predicted present time `t_last + refresh_interval`. Use egui's `stable_dt`/`predicted_dt` until we have a better source. On Android 0.31 winit gives no refresh rate, so we will need a JNI `Choreographer` hook later.
- Never drive animations from wall-clock timers while vsync is working.

### 3.3 Key types (signatures only; original design)

```rust
// ljos_render::tree
pub struct NodeId(u32 /*index*/, u32 /*generation*/);

bitflags::bitflags! {
    pub struct Dirty: u16 {
        const CONTENT   = 1 << 0;  // display list must be rebuilt
        const TRANSFORM = 1 << 1;  // local transform changed → recompose subtree
        const OPACITY   = 1 << 2;
        const CLIP      = 1 << 3;
        const ORDER     = 1 << 4;  // children/z changed → resort paint order
        const BOUNDS    = 1 << 5;  // subtree AABB invalid
        const LAYER     = 1 << 6;  // layer spec or layer contents changed
        const A11Y      = 1 << 7;
        const VISIBLE   = 1 << 8;
    }
}

pub struct RenderNode {
    pub parent: Option<NodeId>,
    pub children: smallvec::SmallVec<[NodeId; 4]>, // declaration order
    pub paint_order: Box<[NodeId]>,                // cached: (z, decl) sorted; negatives first
    pub local: kurbo::Affine,                      // x/y, transformOrigin, rotation, scale, transform[]
    pub world: kurbo::Affine,
    pub size: kurbo::Size,
    pub subtree_bounds: kurbo::Rect,               // local space, includes children
    pub opacity: f32,                              // multiplied down (Qt semantics, not grouped)
    pub clip: Option<ClipSpec>,
    pub layer: Option<LayerSpec>,                  // layer.enabled & friends → offscreen
    pub visible: bool,
    pub content: Option<std::sync::Arc<DisplayList>>,
    pub dirty: Dirty,
    pub last_visible_frame: u64,
}

pub enum ClipSpec { Rect(kurbo::Rect), RoundedRect(kurbo::RoundedRect), Mask(ImageKey) }
pub struct LayerSpec { pub texture_size: Option<(u16, u16)>, pub smooth: bool, pub mipmap: bool,
                       pub effect: Option<EffectKey>, pub format: LayerFormat }

// ljos_render::display_list — backend contract
pub struct DisplayList { pub ops: Box<[DrawOp]>, pub bounds: kurbo::Rect }
pub enum DrawOp {
    Rect      { rect: kurbo::Rect, radii: [f32; 4], fill: Paint, border: Option<Border>, aa: bool },
    Shadow    { rect: kurbo::Rect, radii: [f32; 4], blur: f32, spread: f32, color: Rgba },
    Image     { image: ImageKey, src: kurbo::Rect, dst: kurbo::Rect, fill: FillMode, smooth: bool },
    NinePatch { image: ImageKey, insets: Insets, dst: kurbo::Rect, tile: [TileMode; 2] },
    Glyphs    { run: std::sync::Arc<GlyphRun>, paint: Paint, decorations: Decorations },
    Path      { path: std::sync::Arc<kurbo::BezPath>, style: PathStyle, paint: Paint }, // → vello_gpu/cpu
    Embed     { surface: EmbedId, rect: kurbo::Rect },  // egui child Ui / foreign texture
}
pub enum Paint { Solid(Rgba), Gradient(std::sync::Arc<peniko::Gradient>), Image(ImageKey) }

// Items produce content; never touch GPU.
pub trait PaintContent {
    fn paint(&self, cx: &mut PaintCx<'_>, out: &mut DisplayListBuilder);
}

pub struct ChangeSet { pub dirty_nodes: Vec<NodeId>, pub removed: Vec<NodeId>, pub frame: u64 }

pub struct FrameView<'a> {
    pub tree: &'a RenderTree, pub viewport: kurbo::Rect, pub scale_factor: f32,
    pub visible: &'a [NodeId] /* culled, in paint order, with segment breaks at Embed */,
}

pub trait RenderBackend {
    fn sync(&mut self, tree: &RenderTree, changes: &ChangeSet);
    fn encode(&mut self, view: &FrameView<'_>, sink: &mut EguiSink<'_>) -> EncodeStats;
    fn trim(&mut self, pressure: MemoryPressure);       // Android onTrimMemory, suspend
    fn snapshot_rgba(&mut self, view: &FrameView<'_>) -> Option<image::RgbaImage>; // tests
}
pub struct EguiSink<'a> { pub painter: &'a egui::Painter /* + callback registration */ }

// Time
pub trait FrameClock: Send {
    fn now(&self) -> FrameTime;                 // predicted present time
    fn request_frame(&self);
}
pub trait Animation {
    fn tick(&mut self, t: FrameTime, props: &mut PropertyWriter<'_>) -> AnimState;
}
pub enum AnimState { Running, Finished }

// egui embedding (QML inside egui apps)
pub struct QmlView { /* engine handle, root, backend */ }
impl egui::Widget for &mut QmlView { fn ui(self, ui: &mut egui::Ui) -> egui::Response; }

// Reference/test renderer
pub struct VelloCpuReference { /* vello_cpu::RenderContext, fixed Level, 1 thread */ }
impl RenderBackend for VelloCpuReference { /* … */ }
```

**Paint-order rule**, implemented once in `compose` and shared by paint, hit testing and a11y order:
1. Children with `z < 0`, stable-sorted by `(z, decl_index)`.
2. The node's own content.
3. Children with `z ≥ 0`, stable-sorted the same way.

`visible == false` or `opacity == 0` prunes the subtree from paint and hit testing (keyboard focus stays possible, per Qt docs). Layered subtrees render into their texture in local space, and the parent draws a quad with the layer's opacity.

**Culling:**
- Skip a node when `world.transform_rect_bbox(subtree_bounds) ∩ clip_stack ∩ viewport = ∅`.
- `subtree_bounds` is recomputed bottom-up only along `BOUNDS`-dirty chains.
- Flickable content uses a batch root: in B1 a scroll changes only the root uniform, and in B0 only the per-mesh translation.

### 3.4 Testing plan

1. **Behaviour and a11y**: `egui_kittest::Harness::new_ui(|ui| qml_view.ui(ui))`.
   - Query QML items by AccessKit label and role, click and type, then `run()`.
   - The Ljós `FrameClock` is virtual in tests and advanced by `Harness::step`.
   - B0 shapes snapshot with kittest's wgpu renderer (lavapipe in CI; keep the per-OS thresholds in `kittest.toml`).
2. **Pixel parity with Qt**: render qml-solid's corpus (`corpus/qtdoc` and `corpus/reference/*.png`, made by Qt's `qml` tool with `QT_QPA_PLATFORM=offscreen`, dpr 1, `QT_SCALE_FACTOR=1`) through the **vello_cpu reference backend**.
   - Use a fixed SIMD `Level` and one thread for bit-stable output across machines.
   - Use bundled fonts with identical files and hinting off on both sides.
   - Score with the same two metrics as qml-solid's `examples/gallery/tests/compare.js`: `pixels` (pixelmatch threshold 0.1, AA excluded) and `content` (only non-background pixels).
   - Use per-example **ratcheting floors**, as qml-solid does. Port compare.js to Rust; it is ~60 lines.
3. **Backend parity**: render the same `FrameView` with B0/B1 (wgpu, lavapipe) and R, with tolerance-based diffs. This catches SDF and gradient drift and tessellation regressions.
4. **Performance gates**: a headless bench of 10k-node scenes measuring sync time, encode time, vertices and bytes uploaded per frame, and draw calls. Fail CI on regressions. Provide Qt-like debug env vars: `LJOS_VISUALIZE=batches|clip|changes|overdraw|layers`.

### 3.5 Rejected alternatives (and why)

- **Pure immediate (a)**: wrong semantics and O(total) per-frame cost (§2.4).
- **Making vello (compute) the main renderer**: needs compute shaders, which are unreliable on older Android GLES devices. Slint's docs flag the same constraint, and the crate now sits under `research/`.
- **vello_gpu for *everything***: it re-strips every path every frame with no scene cache. QML's rect-heavy scenes are cheaper as instanced SDF quads. vello_gpu is used where it shines: arbitrary paths, rendered into cached textures.
- **Skia (Freya-style)**: large C++ dependency, slow builds and binary size on Android, and outside our Rust-native stack.
- **cosmic-text**: duplicates fontique and Parley and is weaker on editing and AccessKit integration. Parley shares egui's skrifa and harfrust versions.
- **egui text (`Galley`)**: no bidi and no justification, which conflicts with the localisation goal.
- **Taffy as the core layout engine**: does not match QtQuick.Layouts or anchors semantics. Keep it as an optional extension.
- **Building directly on Masonry**: different reactive model (Xilem), no release for a year, vello-centric. Borrow ideas, not code.

---

## 4. Risks and open questions

1. **Android text input.** winit's Android IME is show/hide only: no preedit, no surrounding text, no `ImePurpose`. HAL-C2's mobile chat needs a full `InputConnection`. Expect to write a JNI bridge (GameActivity's `GameTextInput`) or contribute upstream. This is the largest mobile risk, and it is independent of rendering.
2. **egui churn.** There are breaking releases roughly every 2 months (0.34 → 0.35 → 0.36; 0.37 changes `Id`, accessibility roles and fonts). Keep the egui surface area small (host, input, `Painter`, callback) behind `ljos_egui`.
3. **Full-frame redraws.** egui redraws and re-uploads the whole frame on every input event, including hover. B0 cost scales with visible geometry. Measure on a mid-range Android device (e.g. a 2022 Adreno 6xx or Mali-G57) before committing to the B1 timeline. B1 brings static-content upload to zero but not draw submission.
4. **Interleaving egui widgets with B1.** Each `EguiItem` splits the Ljós draw into callback segments. Many embeds mean many callbacks and pipeline switches. Recommend limiting embeds to leaf "islands".
5. **AccessKit grafting and kittest.** We need to verify that `kittest` 0.5 and `accesskit_consumer` 0.39 resolve grafted subtrees. Otherwise tests must merge trees manually, or B0 must keep the per-frame `accesskit_node_builder` path in tests.
6. **Font stack duplication.** egui 0.37 pins fontique 0.11, while Parley 0.12 uses fontique 0.12. That means two system-font scans and some binary bloat. Ask upstream to align, or give egui a minimal font set.
7. **Pixel parity limits.** Qt's offscreen QPA uses its own rasteriser and text pipeline, so exact matches are impossible for text-heavy examples. The goal is content scores with ratcheting floors, not equality.
8. **Rotated and 3D transforms.** QML `Rotation { axis }` is perspective. B0 can only approximate it (2D affine). B1 needs a 4×4 path for quads and layers.
9. **Render thread on Android.** Surface lifecycle events arrive on the activity thread. A render thread complicates `Suspended`/`Resumed` handling. Keep v1 single-threaded and add the render thread only once profiling shows a need.
10. **Hot reload.** Display lists and GPU caches are derived data, so a code swap only re-runs `PaintContent::paint` for affected nodes. Node identity must survive reloads (keyed by component path plus object id) to keep animation state and textures. This needs to be coordinated with the QML engine report.

---

## 5. Sources

**egui**
- crates.io egui (versions/dates): https://crates.io/crates/egui
- egui CHANGELOG (0.34–0.37 unreleased): https://github.com/emilk/egui/blob/main/CHANGELOG.md
- epaint CHANGELOG: https://github.com/emilk/egui/blob/main/crates/epaint/CHANGELOG.md
- epaint `Shape`, `RectShape`, `TextShape`, `Mesh`, `Vertex`, tessellator: https://github.com/emilk/egui/tree/main/crates/epaint/src
- epaint text layout (bidi TODOs): https://github.com/emilk/egui/blob/main/crates/epaint/src/text/text_layout.rs and https://github.com/emilk/egui/blob/main/crates/epaint/src/text/unicode.rs
- egui `Context` (repaint, layer transforms, accesskit_node_builder, end-of-pass TreeUpdate): https://github.com/emilk/egui/blob/main/crates/egui/src/context.rs
- egui-wgpu `CallbackTrait`, `register_native_texture`, `update_buffers`: https://github.com/emilk/egui/blob/main/crates/egui-wgpu/src/renderer.rs
- `PaintCallback`: https://github.com/emilk/egui/blob/main/crates/epaint/src/shapes/paint_callback.rs
- egui_kittest README and source: https://github.com/emilk/egui/tree/main/crates/egui_kittest
- eframe Android features and hello_android: https://github.com/emilk/egui/tree/main/examples/hello_android
- Workspace dependency pins (wgpu 30, winit 0.30.13, accesskit 0.25, skrifa 0.44, harfrust 0.12, vello_cpu 0.2): https://github.com/emilk/egui/blob/main/Cargo.toml

**Vello, Parley, Masonry**
- Vello README and ARCHITECTURE (vello_cpu, vello_gpu, research vello): https://github.com/linebender/vello and https://github.com/linebender/vello/blob/main/ARCHITECTURE.md
- vello_gpu CHANGELOG (rename from vello_hybrid, 0.3.0): https://github.com/linebender/vello/blob/main/vello_gpu/CHANGELOG.md
- vello_cpu CHANGELOG: https://github.com/linebender/vello/blob/main/vello_cpu/CHANGELOG.md
- Vello CPU thesis (L. Völker, ETH 2025): https://ethz.ch/content/dam/ethz/special-interest/infk/inst-pls/plf-dam/documents/StudentProjects/MasterTheses/2025-Laurenz-Thesis.pdf
- Parley CHANGELOG 0.12.0: https://github.com/linebender/parley/blob/main/CHANGELOG.md
- Masonry pass system: https://github.com/linebender/xilem/blob/main/masonry_core/src/doc/pass_system.md
- Masonry paint pass (per-widget scene cache): https://github.com/linebender/xilem/blob/main/masonry_core/src/passes/paint.rs
- masonry_imaging backends: https://github.com/linebender/xilem/tree/main/masonry_imaging
- Linebender blog feed: https://linebender.org/atom.xml

**AccessKit and winit**
- AccessKit graft nodes / `TreeId`: https://github.com/AccessKit/accesskit/blob/main/accesskit/src/lib.rs and https://docs.rs/accesskit/0.25.1/accesskit/struct.TreeId.html
- winit Android IME implementation: https://github.com/rust-windowing/winit/blob/master/winit-android/src/event_loop.rs
- winit 0.31 changelog: https://github.com/rust-windowing/winit/blob/master/winit/src/changelog/v0.31.md

**Qt and Slint (documentation only)**
- Qt Quick Scene Graph: https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html
- Qt Quick Scene Graph Default Renderer: https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph-renderer.html
- QML Item (opacity, layer, z, clip, transforms): https://doc.qt.io/qt-6/qml-qtquick-item.html
- Qt Quick performance considerations: https://doc.qt.io/qt-6/qtquick-performance.html
- Slint backends and renderers: https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backends_and_renderers/

**Other egui layers and toolkits**
- egui_taffy docs: https://docs.rs/egui_taffy/latest/egui_taffy/
- egui_mobius: https://github.com/saturn77/egui_mobius
- Blitz: https://github.com/DioxusLabs/blitz
- Freya: https://github.com/marc2332/freya
- Iced (wgpu quad pipeline): https://github.com/iced-rs/iced
- Makepad: https://github.com/makepad/makepad
- Zed GPUI, "Leveraging Rust and the GPU to render user interfaces at 120 FPS": https://zed.dev/blog/videogame

**Registry data and local references**
- crates.io API (all version numbers in §2.5, queried 2026-10-10): https://crates.io/api/v1/crates/{name}
- qml-solid gallery comparison (local): `/home/olafura/dev/qml-solid/examples/gallery/tests/compare.js` and `/home/olafura/dev/qml-solid/corpus/reference.mjs`
