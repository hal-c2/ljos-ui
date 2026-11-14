# What a 3D frame costs

How to see what Ljós spends on each frame of a `View3D`, and how to compare it with `qml6` running the same QML. docs/quick3d.md § Performance has the budget; this page covers the instruments.

## The counters

Every frame that draws a scene counts what it did. The renderer adds to a tally on the thread that draws, which costs an add and takes no lock. The host folds the tally into one record once it has submitted (`ljos_3d::stats::frame_end`). Where views are drawn off-screen and read back as pictures, each view's submit adds to the tally, and the host ends the frame once it has drawn them all, so a frame of several views is one record. A frame that draws no scene counts nothing and writes nothing, so a still window stays at 0 % CPU.

| Counter | What it is |
|---|---|
| `views` | views drawn in the frame |
| `draw_calls`, `vertices` | draw calls of every pass (shadows, depth, the scene, post-processing); vertices, each instanced copy counted |
| `render_passes` | render passes begun |
| `pipeline_switches` | `set_pipeline` calls |
| `bind_group_sets` | `set_bind_group` calls |
| `buffer_writes`, `buffer_write_bytes` | writes into buffers, including a buffer made with its contents |
| `texture_uploads`, `texture_upload_bytes` | writes into textures |
| `pipelines_created`, `shader_modules_created`, `buffers_created`, `textures_created`, `bind_groups_created` | GPU objects made for the frame; whatever is made before the first frame counts toward that frame |
| `models_drawn`, `models_culled` | models the camera sees, and models it leaves out |
| `sync_ms` | reading the QML scene into the renderer's |
| `prepare_ms` | walking, culling and sorting, writing uniforms, making pipelines |
| `encode_ms` | recording the passes |
| `submit_ms` | the host's submit. It is 0 under egui-wgpu, whose own submit includes its waits |
| `gpu_ms` | GPU time of the last scene, where the adapter has timestamp queries |

They can be read in three places.

**In QML.** `RenderStatistics` (QtQuick3D.Helpers) has the last frame's counts as properties: `drawCallCount`, `drawVertexCount`, `renderPassCount`, `pipelineSwitches`, `bindGroupSets`, `bufferWrites`, `bufferWriteBytes`, `textureUploads`, `textureUploadBytes`, `pipelinesCreated`, `modelsDrawn`, `modelsCulled`, `syncTime`, `renderPrepareTime`, `encodeTime`, `submitTime`, `renderTime`.

`View3D.renderStats` takes Qt's names:
- `renderTime` is recording plus submitting. Qt's is not split there (its documentation counts preparing and recording in it, and on this machine it includes the swap wait), so compare the sums of the parts, not `renderTime` alone;
- `lastCompletedGpuTime` is the GPU time;
- `pipelineCount` is the number of pipelines made so far.

Both read the frame, not one view: where a window has several views, the counts and times are of all of them together, where Qt's `renderStats` is its view's own.

**Over MCP.** `app_metrics` and the paint report carry `scene3d`: how many 3D frames there have been, what the last one cost, and the largest and latest CPU parts.

**As a file.** With `LJOS_3D_FRAMES=<file>`, every frame is one JSON line in that file, stamped `t_us` (microseconds since the epoch):

```json
{"t_us":1794600000000000,"frame":42,"views":1,"draw_calls":523,"render_passes":6,"pipeline_switches":31,…,"encode_ms":0.41,"submit_ms":0.08,"gpu_ms":1.9}
```

## Benching

```sh
LJOS_CUA_GPU=/dev/dri/renderD128 LJOS_CUA_HOME=$PWD/target/cua-gpu mise run bench:3d
mise run bench:3d quick3d-stress dice --runtime ljos   # some scenes, one runtime
```

`bench:3d` is `mise run bench` with `--frames3d`, over the `bench/quick3d-*.qml` scenes and the corpus's 3D apps, with the defaults `--warm-up --repeat 3`. It does not run the memory bisection.
- `--frames3d` gives each run a frames file of its own.
- The report gets two more tables: the first frame, and the p50 and p99 of each count and CPU part while the scene animates.
- Every frame table now has p99 next to p95.

Without `LJOS_CUA_GPU`, the compositor works in software and qml6 draws on llvmpipe. Ljós then draws its windows on the processor, and its 3D still goes to the GPU when it can open one; see Results. `BENCH_BUILD=0` measures the build already there instead of building first.

Each corpus demo runs as `mise run gallery:run` runs it:
- with its stand-ins (`corpus/standins/<id>`, the QML for the C++ of its own build);
- in the directory its downloaded assets are read from. Run `mise run gallery:assets <id>` first.

A corpus demo that `qml6` cannot run on this machine (corpus/examples.json says why) is measured on Ljós alone, and the probe below leaves it out too. `BENCH_QT` names demos to measure on qml6 anyway. Its default, dice and the FX showroom, are the two that qml6 6.12 runs with their stand-ins.

**Qt's side.** A qml6 frame cannot be counted from outside the process. The bench measures what it can see from outside, for both runtimes:
- startup to the window mapped;
- frame intervals from the compositor;
- CPU;
- PSS.

Then `bench:3d` opens each scene in qml6 once more under `bench/quick3d/renderstats-probe.qml`. The probe loads the scene unchanged and logs every `View3D`'s `renderStats` once a second. It turns on `extendedDataCollectionEnabled` so that Qt counts draws and passes, so that run carries that small cost. `bench/quick3d/renderstats-summary.py` takes the median of each second after the first two and writes `qml6-renderstats.json` next to the report.

## The stress scene

`bench/quick3d-stress.qml` is plain QtQuick3D, so qml6 runs it unchanged. It holds:
- 480 models of five meshes, with a glTF model read through `RuntimeLoader` among them;
- twelve lights, four of which cast shadows;
- three textures, one of them 2048 by 2048;
- mixed materials, some of them blended;
- a camera orbiting under a `NumberAnimation`, so every frame draws everything from a new place.

The textures are generated patterns (`bench/quick3d/gen-stress.py`), which keeps the files small while the textures stay full size on the GPU.

## Results

These were measured on 2026-11-14 with `bench:3d` in the GPU sandbox (AMD, Mesa; Ljós on wgpu's GL backend). The runs used release builds and qml6 6.12, and each number is the median of 3 runs. The machine was shared, with a median load average of 30. Treat the numbers as a ranking, not a budget.

| scene | first frame ms, Ljós / Qt | PSS MiB, Ljós / Qt | idle fps, Ljós / Qt | CPU ms per frame, Ljós / Qt |
|---|---|---|---|---|
| quick3d-cube | 74 / 320 | 133 / 122 | still | still |
| quick3d-spin | 74 / 189 | 124 / 133 | 61 / 61 | 1.43 / 0.95 |
| quick3d-shadows | 92 / 202 | 127 / 141 | 61 / 61 | 1.71 / 1.22 |
| quick3d-post | 79 / 198 | 123 / 139 | 53 / 61 | 2.07 / 1.55 |
| quick3d-instances | 93 / 158 | 129 / 123 | 61 / 61 | 2.20 / 0.85 |
| quick3d-grid | 114 / 238 | 141 / 171 | **41** / 60 | 14.1 / 19.2 |
| quick3d-stress | 122 / 597 | 135 / 291 | 51 / 41 | 8.05 / 30.2 |

The corpus demos, at a median load of 14 (osmbuildings and car-configurator need C++ types that qml6 lacks here, so they ran on Ljós only):

| demo | first frame ms, Ljós / Qt | PSS MiB, Ljós / Qt | idle CPU %, Ljós / Qt | notes |
|---|---|---|---|---|
| dice | 161 / 382 | 146 / 219 | 0 / 70 | once the dice settle, Ljós rests and qml6 keeps its main thread busy |
| fx-material-showroom | **616** / 204 | **229** / 128 | **8.4** / 4.2 | no 3D frame at rest; Ljós's main thread spends 16× Qt's. Its `floor.frag` CustomMaterial fails to compile on Ljós, so the two do not draw the same picture yet |
| osmbuildings | 111 / – | 358 / – | 60 / – | **9.5 fps**, p99 585 ms; sync 12 ms for 177 models, plus JS while tiles arrive |
| car-configurator | 600 / – | 366 / – | **75** / – | the 3D frame is cheap at rest, so the main thread's cost lies elsewhere. Its three `WallEffect` ShaderEffects fail shader preparation on Ljós, so it draws less than the demo does |

Ljós reaches its first frame two to five times sooner than qml6 on every scene but the FX showroom, and it uses less memory once a scene is large.
- On small animated scenes, Ljós spends more CPU per frame than Qt.
- On grid, Ljós cannot hold 60 fps. One moving light makes its whole scene be read again. Of its 12 ms 3D frame:
  - `sync_ms` is 6 ms, where Qt's `syncTime` is 0.02 ms;
  - `submit_ms` is 4 ms, a thousand draws replayed into GL.
- stress favours Ljós partly because Ljós keeps its shadow maps while nothing moves, where Qt draws them every frame: 1 pass against 12.

Callgrind of grid and stress shows where a frame goes (valgrind's `callgrind`, counters zeroed after warm-up, 60 s collected):
- about 41 % reading the scene, most of it its materials;
- 18 % preparing, a third of which is sorting the draws;
- 20 % in wgpu's encode and submit;
- the rest in the driver.

**On llvmpipe** the ranking changes. Without `LJOS_CUA_GPU` the sandbox's compositor works in software, so Ljós draws its windows on the processor. Its 3D views go to `ljos_3d::offscreen` on wgpu's first adapter, and that is still the GPU wherever a render node can be opened. To hold Ljós to llvmpipe as well, add `--app-env WGPU_BACKEND=gl --app-env LIBGL_ALWAYS_SOFTWARE=1`. Measured that way (median load 17; qml6 from the plain run, where it already draws on llvmpipe):
- instances, post and shadows drop to 37–52 fps, where Qt holds 62. They spend 1.2–1.6× Qt's CPU per frame.
- stress's lead goes away: 8.2 fps against 6.1 at the same CPU, because the off-screen path draws its shadow maps every frame (10 passes against 1).
- grid falls to 35.5 fps, against Qt's 52.8.
- The FX showroom takes 14.5 s to its first frame, against Qt's 0.3 s.
- dice and the other startups keep their order.

What to fix, in order:
1. **Read only what changed into the scene** instead of building it again: `ljos_quick3d::view::read` calls `scene::build` for any change. A first step is to cache each material, since a material that does not change need not be read every frame.
2. **Sort small keys, not draws.** A `Draw` carries its whole model uniform, about 770 bytes, so sorting a thousand of them by depth copies them several times. Keep the draw lists between frames.
3. **Fewer draws and bind-group sets.** On GL, wgpu replays every one at submit. Draw models that share a mesh and material as one instanced draw.
4. **Keep each model in the same uniform slot.** Today the slots follow the depth order, so a moving camera rewrites them all.
5. **Leave instance rows alone when only their parent moves.**
6. **Read a skin's inverse bind poses once,** not through JS on every build. On toycustomizer that is half the frame.
7. **Keep the off-screen path's targets.** `Offscreen::render` makes a new target for every picture: 8–10 buffers, 2–3 textures and their shadow maps each frame. Then it waits for the read-back. Keep one target per view and map last frame's picture.
8. **Keep a light probe's pixels until it is uploaded.** While car-configurator loads, `Ibl::prepare_texture` asks for the probe's pixels, which were let go after decoding, so the whole KTX file is decoded again on the main thread inside the frame. Its `ktx::Pixels::at` also calls `powi` for every channel of every sample (15 % of the load under callgrind).
9. **Find what the FX showroom and car-configurator do at rest.** The showroom draws no 3D frame then and car-configurator's is cheap, yet both main threads stay busy. osmbuildings draws too slowly under callgrind to profile.
