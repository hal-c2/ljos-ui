//! What the shell draws a platform window with: the graphics device all
//! windows share, a painter for each, and the surface of a window that
//! is open.
//!
//! The surface belongs to the window: it is made when the window opens
//! and goes before the window does. The painter stays with the pictures
//! it was given, so a window that is hidden and shown again draws at
//! once.

use std::num::NonZeroU32;
use std::sync::{Arc, mpsc};

use egui::ViewportId;
use egui_wgpu::winit::Painter;
use egui_wgpu::{
    RendererOptions, SurfaceConfig, WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew,
    WgpuSetupExisting, wgpu,
};
use egui_winit::winit::event_loop::OwnedDisplayHandle;
use egui_winit::winit::window::Window;
use ljos_host::{WindowId, WindowState};

/// The device the first window was given. Every later window draws with
/// the same one: a second device would be a second set of everything.
#[derive(Clone)]
pub(crate) struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu").finish_non_exhaustive()
    }
}

impl Gpu {
    /// The device of a painter that has drawn into a window; `None`
    /// before its first window.
    pub(crate) fn of(painter: &Painter) -> Option<Gpu> {
        let state = painter.render_state()?;
        Some(Gpu {
            instance: state.instance,
            adapter: state.adapter,
            device: state.device,
            queue: state.queue,
        })
    }
}

/// The graphics instance being made while the application starts, on a
/// thread of its own. Making it loads and starts the drivers, which takes
/// longer than building most applications (hundreds of milliseconds for
/// EGL on Mesa, more for Vulkan's loader and its layers); the first window
/// only waits for what is left of it. One instance for every backend, as
/// the painter makes it: drivers started side by side, an instance for
/// each on a thread of its own, crashed and hung the process.
pub(crate) struct Starting {
    instance: mpsc::Receiver<wgpu::Instance>,
    setup: WgpuSetupCreateNew,
    /// Waited for when the application ends before its first window: a
    /// process that ends while a driver is starting can hang in its exit.
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Starting {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl std::fmt::Debug for Starting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Starting")
            .field("backends", &self.setup.instance_descriptor.backends)
            .finish_non_exhaustive()
    }
}

impl Starting {
    /// Starts making the instance wgpu would be asked for.
    pub(crate) fn start(display: OwnedDisplayHandle) -> Self {
        let setup = create_new(display);
        let (tx, instance) = mpsc::sync_channel(1);
        let new = WgpuSetup::CreateNew(setup.clone());
        // Without the thread the first window's painter makes the instance.
        let thread = std::thread::Builder::new()
            .name("ljos-gpu".into())
            .spawn(move || {
                let _ = tx.send(pollster::block_on(new.new_instance()));
            })
            .ok();
        Self {
            instance,
            setup,
            thread,
        }
    }

    /// The device for `window`: the adapter wgpu ranks first among those
    /// that can draw into it. `None` when the instance could not be made
    /// or has none.
    pub(crate) fn device_for(self, window: &Arc<Window>) -> Option<Gpu> {
        let instance = self.instance.recv().ok()?;
        let surface = instance.create_surface(Arc::clone(window)).ok()?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: self.setup.power_preference,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        }));
        // The painter makes the window's own surface.
        drop(surface);
        let adapter = adapter.ok()?;
        let descriptor = (self.setup.device_descriptor)(&adapter);
        let (device, queue) = pollster::block_on(adapter.request_device(&descriptor)).ok()?;
        Some(Gpu {
            instance,
            adapter,
            device,
            queue,
        })
    }
}

/// How a device is asked for: every backend wgpu was built with (or those
/// `WGPU_BACKEND` names), on the window system's display.
fn create_new(display: OwnedDisplayHandle) -> WgpuSetupCreateNew {
    let mut new = WgpuSetupCreateNew::from_display_handle(display);
    // A debug build turns wgpu's validation on, and on GL that hands
    // EGL a debug callback which wgpu-hal 30.0.1 lets read a null
    // command name: Mesa reports a failed eglInitialize through it
    // and the process segfaults as it starts. WGPU_VALIDATION=1
    // still asks for it.
    if std::env::var_os("WGPU_VALIDATION").is_none() {
        new.instance_descriptor
            .flags
            .remove(wgpu::InstanceFlags::VALIDATION);
    }
    new
}

/// A painter for the windows of `ctx`. The first one (`gpu` is `None`)
/// picks the device when it is given its window.
pub(crate) fn painter(
    ctx: &egui::Context,
    gpu: Option<&Gpu>,
    display: OwnedDisplayHandle,
) -> Painter {
    let wgpu_setup = match gpu {
        Some(gpu) => WgpuSetup::Existing(WgpuSetupExisting {
            instance: gpu.instance.clone(),
            adapter: gpu.adapter.clone(),
            device: gpu.device.clone(),
            queue: gpu.queue.clone(),
        }),
        None => WgpuSetup::CreateNew(create_new(display)),
    };
    let config = WgpuConfiguration {
        surface: SurfaceConfig {
            // ARCHITECTURE.md §7.2 step 12: a frame waits for the display
            // and none is torn or dropped. It is also what keeps a window
            // that animates from drawing more often than it is shown.
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: Some(2),
        },
        wgpu_setup,
        ..WgpuConfiguration::default()
    };
    let options = RendererOptions {
        // B0 antialiases its own edges (feathering).
        msaa_samples: 1,
        ..RendererOptions::default()
    };
    // A surface that can be seen through, for every window: whether a
    // window is, is said when the window is opened (and an opaque one
    // clears to an opaque colour), while the painter outlives it.
    let see_through = true;
    pollster::block_on(Painter::new(ctx.clone(), config, see_through, options))
}

/// An open platform window and what egui needs to be shown in it.
pub(crate) struct Surface {
    pub(crate) id: WindowId,
    pub(crate) window: Arc<Window>,
    /// egui's side of the window: input, the input method, the clipboard
    /// and the AccessKit adapter.
    pub(crate) state: egui_winit::State,
    pub(crate) info: egui::ViewportInfo,
    /// Opened unseen; shown once there is a picture in it.
    pub(crate) shown: bool,
    /// The size in physical pixels the painter was last told.
    px: Option<[u32; 2]>,
}

impl std::fmt::Debug for Surface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Surface")
            .field("id", &self.id)
            .field("shown", &self.shown)
            .finish_non_exhaustive()
    }
}

impl Surface {
    pub(crate) fn new(id: WindowId, window: Arc<Window>, state: egui_winit::State) -> Self {
        Self {
            id,
            window,
            state,
            info: egui::ViewportInfo::default(),
            shown: false,
            px: None,
        }
    }

    /// The window's size in points of `ctx`, and the painter is told when
    /// the size in pixels is another than it knows. `None` while the
    /// window has no area (minimized on some platforms).
    pub(crate) fn fit(&mut self, ctx: &egui::Context, painter: &mut Painter) -> Option<[f64; 2]> {
        let px = self.window.inner_size();
        let (w, h) = (NonZeroU32::new(px.width)?, NonZeroU32::new(px.height)?);
        if self.px != Some([px.width, px.height]) {
            self.px = Some([px.width, px.height]);
            painter.on_window_resized(ViewportId::ROOT, w, h);
        }
        let ppp = f64::from(egui_winit::pixels_per_point(ctx, &self.window));
        Some([f64::from(px.width) / ppp, f64::from(px.height) / ppp])
    }

    /// How the platform shows the window now.
    pub(crate) fn shown_as(&self) -> WindowState {
        if self.window.fullscreen().is_some() {
            WindowState::FullScreen
        } else if self.window.is_minimized() == Some(true) {
            WindowState::Minimized
        } else if self.window.is_maximized() {
            WindowState::Maximized
        } else {
            WindowState::Normal
        }
    }
}

/// Drops from `pending` the pictures that were made and thrown away
/// again before any window could be given them. A window that is not on
/// the screen keeps what it would have to upload; without this an
/// application that never shows it would keep everything it ever drew.
pub(crate) fn forget_freed(pending: &mut egui::TexturesDelta) {
    if pending.free.is_empty() {
        return;
    }
    let free = &pending.free;
    // The frees stay: the painter may hold an earlier upload of them.
    pending.set.retain(|id, _| !free.contains(id));
}

/// Gives the GPU memory of what `pending` frees back now, for a window
/// that is not on the screen: its next picture may be far off, and a
/// trimmed view frees most of what it drew (ARCHITECTURE §8.5).
pub(crate) fn free_now(painter: Option<&Painter>, pending: &mut egui::TexturesDelta) {
    let Some(state) = painter.and_then(Painter::render_state) else {
        return;
    };
    if pending.free.is_empty() {
        return;
    }
    let mut renderer = state.renderer.write();
    for id in std::mem::take(&mut pending.free) {
        renderer.free_texture(&id);
    }
}

#[cfg(test)]
mod tests {
    use egui::epaint::ImageDelta;
    use egui::{ColorImage, TextureId, TextureOptions};

    use super::*;

    fn picture() -> ImageDelta {
        ImageDelta::full(
            ColorImage::filled([1, 1], egui::Color32::WHITE),
            TextureOptions::LINEAR,
        )
    }

    #[test]
    fn a_picture_thrown_away_before_it_was_shown_is_not_kept() {
        let mut pending = egui::TexturesDelta::default();
        pending.push(TextureId::Managed(1), picture());
        pending.push(TextureId::Managed(2), picture());
        pending.free(TextureId::Managed(1));
        forget_freed(&mut pending);
        let kept: Vec<_> = pending.set.keys().copied().collect();
        assert_eq!(kept, vec![TextureId::Managed(2)]);
        assert!(
            pending.free.contains(&TextureId::Managed(1)),
            "the painter may have an earlier upload to drop"
        );
        pending.clear();
    }

    #[test]
    fn pictures_still_in_use_are_all_kept() {
        let mut pending = egui::TexturesDelta::default();
        pending.push(TextureId::Managed(1), picture());
        forget_freed(&mut pending);
        assert_eq!(pending.set.len(), 1);
        pending.clear();
    }
}
