//! A scene drawn into a surface of its own: the embedding for a host that
//! has no egui, such as a `<canvas>` on a web page or a native window of an
//! application's own.
//!
//! The host gives a [`wgpu::SurfaceTarget`] (on the web
//! `SurfaceTarget::Canvas` or `SurfaceTarget::OffscreenCanvas`) and its
//! size in device pixels, and calls [`SurfaceView::draw`] when it has a
//! scene to show. Such a host has no 2D painter whose device it could
//! share, so the first view asks for a [`Gpu`]; every further view on the
//! page or in the application draws on that one through
//! [`SurfaceView::with_gpu`], so the meshes, textures and pipelines the
//! views have in common are made once.
//!
//! The view has no frame loop and no timer: it draws only when the host
//! calls it, and then only if the scene's revision or the size changed. The
//! surface keeps showing the last picture, so a still scene costs nothing
//! and a host may call `draw` from every frame callback it already has.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::gpu::{Renderer, Target};
use crate::scene::Scene;

#[derive(Debug, thiserror::Error)]
pub enum SurfaceViewError {
    #[error("the surface could not be made: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("no graphics adapter for the surface: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("no graphics device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("the adapter cannot draw into this surface")]
    Unsupported,
}

/// A graphics device and the [`Renderer`] of it, shared by every view that
/// draws on it. Cloning it shares it.
///
/// The renderer is shared and not one per view because what it keeps (the
/// meshes, textures, probes and programs) is let go when no scene holds it
/// any more, not when one view stops drawing it, so two views showing the
/// same model hold one copy of it.
#[derive(Clone)]
pub struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Behind a lock so the views can share it and stay `Send` where wgpu
    /// is; views draw one at a time, so nothing waits on it.
    renderer: Arc<Mutex<Renderer>>,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu")
            .field("adapter", &self.adapter.get_info().name)
            .field("backend", &self.backend())
            .finish_non_exhaustive()
    }
}

impl Gpu {
    /// A device on the first adapter wgpu ranks, with no surface to be
    /// compatible with: for a native host, or a page whose views all draw
    /// with WebGPU. A view made with [`SurfaceView::new`] asks for one
    /// compatible with its surface instead, which WebGL2 needs; take that
    /// one from [`SurfaceView::gpu`] for the next views.
    pub async fn new() -> Result<Self, SurfaceViewError> {
        Gpu::request(Gpu::instance().await, None).await
    }

    async fn instance() -> wgpu::Instance {
        wgpu::util::new_instance_with_webgpu_detection(
            wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
        )
        .await
    }

    async fn request(
        instance: wgpu::Instance,
        compatible: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, SurfaceViewError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: compatible,
                ..Default::default()
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("ljos_3d surface"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await?;
        let mut renderer = Renderer::new(&device);
        renderer.use_adapter_formats(&adapter);
        Ok(Gpu {
            instance,
            adapter,
            device,
            queue,
            renderer: Arc::new(Mutex::new(renderer)),
        })
    }

    /// `BrowserWebGpu` or `Gl` (WebGL2) on the web.
    pub fn backend(&self) -> wgpu::Backend {
        self.adapter.get_info().backend
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// The renderer, for a host that also draws into [`Target`]s of its
    /// own. Held only while drawing: a view drawing waits for it.
    pub fn renderer(&self) -> MutexGuard<'_, Renderer> {
        // A view that panicked mid-draw left caches a later draw rebuilds,
        // not a renderer that cannot draw.
        self.renderer.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct SurfaceView<'w> {
    surface: wgpu::Surface<'w>,
    gpu: Gpu,
    config: wgpu::SurfaceConfiguration,
    /// The surface's format without its sRGB suffix: the picture is already
    /// sRGB and is copied, not encoded again.
    view_format: wgpu::TextureFormat,
    /// Kept for [`SurfaceView::backend`]: a web host has to know whether
    /// it got WebGPU or fell back to WebGL2, which nothing else tells it.
    backend: wgpu::Backend,
    target: Target,
    presented: Option<u64>,
}

impl std::fmt::Debug for SurfaceView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceView")
            .field("size", &[self.config.width, self.config.height])
            .field("format", &self.config.format)
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

impl<'w> SurfaceView<'w> {
    /// A view over `target`, `width` by `height` device pixels (on the web
    /// the canvas's CSS size times `devicePixelRatio`; the view sets the
    /// canvas's `width` and `height` to it), on a [`Gpu`] of its own that
    /// can draw into it. On the web it takes WebGPU where the browser has
    /// it and an adapter answers, and WebGL2 where not; natively the
    /// backends wgpu's environment variables allow.
    ///
    /// Asynchronous because a browser answers for adapters and devices only
    /// through promises; a native host may block on it.
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'w>>,
        width: u32,
        height: u32,
    ) -> Result<Self, SurfaceViewError> {
        let instance = Gpu::instance().await;
        let surface = instance.create_surface(target)?;
        let gpu = Gpu::request(instance, Some(&surface)).await?;
        SurfaceView::on(gpu, surface, width, height)
    }

    /// A view over `target` drawing on `gpu`, which it shares with the
    /// other views on it. Fails as [`SurfaceViewError::Unsupported`] where
    /// the device cannot present to the surface: under WebGL2 a device
    /// draws into the one canvas it was made for.
    pub fn with_gpu(
        gpu: &Gpu,
        target: impl Into<wgpu::SurfaceTarget<'w>>,
        width: u32,
        height: u32,
    ) -> Result<Self, SurfaceViewError> {
        let surface = gpu.instance.create_surface(target)?;
        SurfaceView::on(gpu.clone(), surface, width, height)
    }

    fn on(
        gpu: Gpu,
        surface: wgpu::Surface<'w>,
        width: u32,
        height: u32,
    ) -> Result<Self, SurfaceViewError> {
        let backend = gpu.backend();
        let caps = surface.get_capabilities(&gpu.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .ok_or(SurfaceViewError::Unsupported)?;
        let view_format = format.remove_srgb_suffix();
        // Premultiplied where the compositor takes it, so a transparent
        // background shows the page or desktop under the view.
        let alpha_mode = if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            wgpu::CompositeAlphaMode::Auto
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: width.max(1),
            height: height.max(1),
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: if view_format == format {
                vec![]
            } else {
                vec![view_format]
            },
        };
        surface.configure(&gpu.device, &config);
        let target = {
            let mut renderer = gpu.renderer();
            renderer.prepare_blit(&gpu.device, view_format);
            Target::new(&gpu.device, &renderer, [config.width, config.height])
        };
        Ok(SurfaceView {
            surface,
            gpu,
            config,
            view_format,
            backend,
            target,
            presented: None,
        })
    }

    /// The device the view draws on, to make the next views on it with
    /// [`with_gpu`](Self::with_gpu).
    pub fn gpu(&self) -> &Gpu {
        &self.gpu
    }

    /// The backend the view draws with: `BrowserWebGpu` or `Gl` (WebGL2)
    /// on the web.
    pub fn backend(&self) -> wgpu::Backend {
        self.backend
    }

    /// Resizes the surface to `width` by `height` device pixels. The next
    /// [`draw`](Self::draw) draws whatever the revision; until then the
    /// surface shows nothing useful, so a host draws in the same frame.
    pub fn resize(&mut self, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if [width, height] == [self.config.width, self.config.height] {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.gpu.device, &self.config);
        self.target = Target::new(&self.gpu.device, &self.gpu.renderer(), [width, height]);
        self.presented = None;
    }

    /// Draws and presents `scene` if it changed since the last present or
    /// the size did, and returns whether it presented. An unchanged scene
    /// costs a comparison and no GPU work. Revisions are unique across
    /// scenes, so a host may hand it another scene at any time.
    ///
    /// A surface that is lost or out of date is configured again and drawn
    /// in the same call, because a host that draws only on change would not
    /// call again. One that timed out or is hidden is not drawn; the next
    /// call draws it.
    pub fn draw(&mut self, scene: &Scene) -> bool {
        if self.presented == Some(scene.revision()) {
            return false;
        }
        let Some(frame) = self.frame() else {
            return false;
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.view_format),
            ..Default::default()
        });
        let gpu = &self.gpu;
        let mut renderer = gpu.renderer();
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        renderer.render(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            scene,
            &mut self.target,
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ljos_3d present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            renderer.blit(&mut pass, self.view_format, &self.target);
        }
        drop(renderer);
        gpu.queue.submit([encoder.finish()]);
        gpu.queue.present(frame);
        self.presented = Some(scene.revision());
        true
    }

    fn frame(&mut self) -> Option<wgpu::SurfaceTexture> {
        for _ in 0..2 {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t)
                | wgpu::CurrentSurfaceTexture::Suboptimal(t) => return Some(t),
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    self.surface.configure(&self.gpu.device, &self.config);
                }
                wgpu::CurrentSurfaceTexture::Timeout
                | wgpu::CurrentSurfaceTexture::Occluded
                | wgpu::CurrentSurfaceTexture::Validation => return None,
            }
        }
        None
    }
}
