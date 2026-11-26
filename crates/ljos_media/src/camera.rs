//! `Camera`, `CameraDevice` and `CaptureSession`: pictures from one of the
//! system's cameras, shown in a `VideoOutput` (a viewfinder) and handed to
//! the program's Rust ([`tap_camera`]) as they are captured.
//!
//! The system's cameras are a [`CameraService`]'s, installed per engine
//! with [`set_cameras`]. Without one the host's own is used: V4L2 on
//! 64-bit Linux (the `v4l2` feature), none elsewhere yet ([`NoCameras`]). A test
//! installs [`FakeCameras`], whose pictures it gives itself.
//!
//! A capture runs on the service's thread. Each picture goes, on that
//! thread, first to every tap and then, through a
//! [`crate::VideoFrameInput`], to the output a `CaptureSession` joined the
//! camera to: converted there, and taken by the Engine thread one job a
//! picture at most. A camera lost while it captures says so from that
//! thread through the platform's inbox. Nothing runs while no camera is
//! active.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use ljos_engine::{Atom, Cx, Engine, EngineError, ObjectId, QmlObject, QtObject, Value, qml_impl};
use ljos_platform::{Permission, PermissionStatus, Platform, Post};

use crate::frame_input::{VideoFrame, VideoFrameInput};
use crate::lock;

/// What Qt says of a Camera on a system with no camera (measured, Qt
/// 6.11).
const NO_CAMERA: &str = "No camera detected";
const NOT_PERMITTED: &str = "The camera permission is not granted";

/// Where a camera faces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraPosition {
    #[default]
    Unspecified,
    Back,
    Front,
}

impl CameraPosition {
    fn to_qt(self) -> i32 {
        match self {
            CameraPosition::Unspecified => 0,
            CameraPosition::Back => 1,
            CameraPosition::Front => 2,
        }
    }
}

/// One of the system's cameras, as its service names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CameraInfo {
    /// What the service opens it by; unique among its cameras.
    pub id: String,
    pub description: String,
    pub position: CameraPosition,
    /// The camera a `Camera` uses unless told otherwise.
    pub is_default: bool,
}

/// A capture that runs until it is dropped: dropping it stops the camera.
pub struct Capture(#[allow(dead_code)] Box<dyn std::any::Any>);

impl Capture {
    /// A capture that stops when `running` is dropped.
    pub fn new(running: impl std::any::Any) -> Self {
        Capture(Box::new(running))
    }
}

impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Capture")
    }
}

/// The system's cameras. Asked on the Engine thread; a capture hands its
/// pictures to [`CameraFrames`] from any thread.
pub trait CameraService {
    /// The cameras there are now.
    fn cameras(&self) -> Vec<CameraInfo>;

    /// Starts capturing from `camera`, until the [`Capture`] is dropped.
    ///
    /// # Errors
    /// Why the camera cannot capture, as the user is told it
    /// (`Camera.errorString`).
    fn open(&self, camera: &CameraInfo, frames: CameraFrames) -> Result<Capture, String>;
}

/// A system with no cameras.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoCameras;

impl CameraService for NoCameras {
    fn cameras(&self) -> Vec<CameraInfo> {
        Vec::new()
    }

    fn open(&self, _camera: &CameraInfo, _frames: CameraFrames) -> Result<Capture, String> {
        Err(NO_CAMERA.to_owned())
    }
}

#[derive(Default)]
struct Installed(RefCell<Option<Rc<dyn CameraService>>>);

/// Installs the system's cameras for `engine`. Cameras made before keep
/// the device they chose.
pub fn set_cameras(engine: &Engine, service: Rc<dyn CameraService>) {
    *engine.extension::<Installed>().0.borrow_mut() = Some(service);
}

/// The cameras of `engine`: those installed, or the host's own.
pub fn cameras(engine: &Engine) -> Rc<dyn CameraService> {
    let installed = engine.extension::<Installed>();
    let mut slot = installed.0.borrow_mut();
    Rc::clone(slot.get_or_insert_with(host_cameras))
}

fn host_cameras() -> Rc<dyn CameraService> {
    #[cfg(all(feature = "v4l2", target_os = "linux", target_pointer_width = "64"))]
    return Rc::new(crate::v4l2::V4l2Cameras);
    #[cfg(target_os = "android")]
    return Rc::new(crate::android_camera::AndroidCameras);
    #[cfg(not(any(
        all(feature = "v4l2", target_os = "linux", target_pointer_width = "64"),
        target_os = "android"
    )))]
    Rc::new(NoCameras)
}

// ------------------------------------------------------------ frames

/// A tap of Rust's, and the number its [`CameraTap`] takes it away by.
type Tap = (u64, Box<dyn FnMut(&VideoFrame) + Send>);

/// What a camera object shares with the threads its captures run on.
#[derive(Default)]
struct Shared {
    /// The capture whose pictures are wanted; 0 while none is.
    current: AtomicU64,
    taps: Mutex<Vec<Tap>>,
    /// The output a session joined the camera to, and that session: only
    /// it takes the camera from the output again.
    preview: Mutex<Option<(ObjectId, VideoFrameInput)>>,
}

static NEXT: AtomicU64 = AtomicU64::new(1);

/// Where one capture hands its pictures, from any thread. Clones hand
/// them to the same place. Pictures of a capture that was stopped go
/// nowhere.
#[derive(Clone)]
pub struct CameraFrames {
    shared: Arc<Shared>,
    capture: u64,
    camera: ObjectId,
    post: Post,
}

impl std::fmt::Debug for CameraFrames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CameraFrames")
            .field("camera", &self.camera)
            .field("live", &self.is_live())
            .finish()
    }
}

impl CameraFrames {
    /// Whether the camera still wants this capture's pictures: a service
    /// may stop working on pictures once it does not.
    pub fn is_live(&self) -> bool {
        self.shared.current.load(Ordering::Acquire) == self.capture
    }

    /// Hands `frame` to the camera's taps, then to the output it is shown
    /// in, here on the calling thread.
    pub fn frame(&self, frame: VideoFrame) {
        if !self.is_live() {
            return;
        }
        for (_, tap) in lock(&self.shared.taps).iter_mut() {
            tap(&frame);
        }
        let preview = lock(&self.shared.preview).as_ref().map(|(_, i)| i.clone());
        if let Some(input) = preview {
            // A picture that cannot be converted is not shown; the taps
            // had it all the same.
            let _ = input.send(frame);
        }
    }

    /// The camera stopped capturing by itself (unplugged, taken by
    /// another application); `why` is what the user is told.
    pub fn fail(&self, why: impl Into<String>) {
        if !self.is_live() {
            return;
        }
        let why = why.into();
        let (camera, capture) = (self.camera, self.capture);
        self.post
            .send(move |e: &Engine| lost(e, camera, capture, &why));
    }
}

/// A tap on a camera's pictures, given each on the thread that captured
/// it, until the tap is dropped.
pub struct CameraTap {
    shared: std::sync::Weak<Shared>,
    id: u64,
}

impl std::fmt::Debug for CameraTap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CameraTap").field("id", &self.id).finish()
    }
}

impl Drop for CameraTap {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            // Dropped once the lock is let go: a tap may own another
            // `CameraTap` of this camera, whose drop takes the lock too.
            let removed = {
                let mut taps = lock(&shared.taps);
                taps.iter()
                    .position(|(id, _)| *id == self.id)
                    .map(|at| taps.remove(at))
            };
            drop(removed);
        }
    }
}

/// Why a camera could not be tapped.
#[derive(Debug, thiserror::Error)]
pub enum CameraTapError {
    #[error("{0:?} is no Camera")]
    NotCamera(ObjectId),
}

/// Gives every picture `camera` (a `Camera`) captures to `tap`, on the
/// capture's thread, before it is shown, for as long as the returned
/// [`CameraTap`] lives; across `stop()` and `start()` too. A tap that
/// takes long holds the camera's next picture back, and stopping the
/// camera too, which waits on the capture's thread (on the Engine
/// thread, for V4L2): one that decodes (a QR code, say) hands the frame
/// to a thread of its own. A tap must
/// not drop a [`CameraTap`] of the same camera from inside.
///
/// # Errors
/// `camera` is not a `Camera`.
pub fn tap_camera(
    engine: &Engine,
    camera: ObjectId,
    tap: impl FnMut(&VideoFrame) + Send + 'static,
) -> Result<CameraTap, CameraTapError> {
    let shared = engine
        .with::<Camera, _>(camera, |c| Arc::clone(&c.run.shared))
        .map_err(|_| CameraTapError::NotCamera(camera))?;
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    lock(&shared.taps).push((id, Box::new(tap)));
    Ok(CameraTap {
        shared: Arc::downgrade(&shared),
        id,
    })
}

// ------------------------------------------------------------ CameraDevice

fn device_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Position",
        vec![
            ("UnspecifiedPosition", 0),
            ("BackFace", 1),
            ("FrontFace", 2),
        ],
    )]
}

/// `cameraDevice` (QtMultimedia): one camera, as the system names it.
/// Qt's is a value type; here it is an uncreatable object, one per camera
/// the engine has seen, so `camera.cameraDevice ===
/// devices.defaultVideoInput` holds as it does in Qt.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtMultimedia",
    name = "CameraDevice",
    extends = QtObject,
    enums = device_enums,
    uncreatable
)]
pub struct CameraDevice {
    #[qml(base)]
    pub object: QtObject,
    #[qml(readonly, constant)]
    pub id: Atom,
    #[qml(readonly, constant)]
    pub description: Atom,
    #[qml(readonly, constant)]
    pub is_default: bool,
    #[qml(readonly, constant, enumeration = "Position")]
    pub position: i32,
    #[qml(skip)]
    info: Option<CameraInfo>,
}

#[qml_impl]
impl CameraDevice {}

/// The devices this engine has handed out, by camera id, with what the
/// service said of the camera then; `None` is the device of no camera.
#[derive(Default)]
struct Known(RefCell<HashMap<Option<String>, Handed>>);

/// A device, and what its camera was when it was made.
type Handed = (ObjectId, Option<CameraInfo>);

fn device_of(e: &Engine, info: Option<&CameraInfo>) -> Option<ObjectId> {
    let known = e.extension::<Known>();
    let key = info.map(|i| i.id.clone());
    // A device's properties are constant, so a camera the service now
    // describes otherwise (the default since the one before it went) is
    // a new device.
    let cached = known
        .0
        .borrow()
        .get(&key)
        .filter(|(id, was)| e.is_alive(*id) && was.as_ref() == info)
        .map(|(id, _)| *id);
    if cached.is_some() {
        return cached;
    }
    let device = match info {
        Some(i) => CameraDevice {
            object: QtObject::default(),
            id: Atom::from(i.id.as_str()),
            description: Atom::from(i.description.as_str()),
            is_default: i.is_default,
            position: i.position.to_qt(),
            info: Some(i.clone()),
        },
        None => CameraDevice::default(),
    };
    let id = e.create_native_incomplete(device, None).ok()?;
    let _ = e.complete(id);
    known.0.borrow_mut().insert(key, (id, info.cloned()));
    Some(id)
}

/// The system's cameras as devices, and the default among them (the
/// device of no camera when there is none).
pub(crate) fn video_inputs(e: &Engine) -> (Vec<Value>, Option<ObjectId>) {
    let all = cameras(e).cameras();
    let inputs = all
        .iter()
        .filter_map(|i| device_of(e, Some(i)))
        .map(Value::Object)
        .collect();
    let default = all.iter().find(|i| i.is_default).or(all.first());
    (inputs, device_of(e, default))
}

// ------------------------------------------------------------ Camera

fn camera_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![("Error", vec![("NoError", 0), ("CameraError", 1)])]
}

/// `Camera.Error`.
mod error {
    pub const NONE: i32 = 0;
    pub const CAMERA: i32 = 1;
}

/// `Camera` (QtMultimedia).
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtMultimedia",
    name = "Camera",
    extends = QtObject,
    enums = camera_enums
)]
pub struct Camera {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, write = Self::want_active)]
    pub active: bool,
    #[qml(notify, object = "CameraDevice")]
    pub camera_device: Option<ObjectId>,
    #[qml(readonly, enumeration = "Error")]
    pub error: i32,
    #[qml(readonly)]
    pub error_string: Atom,
    #[qml(skip)]
    run: Run,
}

#[derive(Default)]
struct Run {
    shared: Arc<Shared>,
    capture: Option<Capture>,
    completed: Cell<bool>,
}

impl std::fmt::Debug for Run {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Run")
            .field("capturing", &self.capture.is_some())
            .finish_non_exhaustive()
    }
}

#[qml_impl]
impl Camera {
    #[qml(signal, name = "errorChanged")]
    fn error_changed();
    #[qml(signal, name = "errorOccurred")]
    fn error_occurred(error: i32, error_string: Atom);

    #[qml(invokable)]
    fn start(cx: &Cx) -> Result<(), EngineError> {
        cx.set(Camera::ACTIVE, true)
    }

    #[qml(invokable)]
    fn stop(cx: &Cx) -> Result<(), EngineError> {
        cx.set(Camera::ACTIVE, false)
    }

    #[qml(invokable, name = "setActive")]
    fn set_active_js(cx: &Cx, on: bool) -> Result<(), EngineError> {
        cx.set(Camera::ACTIVE, on)
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        if cx.get(Camera::CAMERA_DEVICE)?.is_none() {
            cx.set(Camera::CAMERA_DEVICE, video_inputs(e).1)?;
        }
        if chosen(e, me).is_none() {
            // Qt says so as the camera is made, before anything listens.
            fail_quietly(e, me, NO_CAMERA);
        }
        e.watch(me, Camera::CAMERA_DEVICE.index, move |e, _| {
            if halt(e, me) {
                restart(e, me);
            }
        })?;
        cx.with::<Camera, _>(|c| c.run.completed.set(true))?;
        if cx.get(Camera::ACTIVE)? {
            restart(e, me);
        }
        Ok(())
    }
}

impl Camera {
    /// Every write of `active` starts or stops the camera; one that
    /// cannot start leaves it inactive, unheard, as in Qt.
    fn want_active(cx: &Cx, on: bool) -> Option<bool> {
        let (e, me) = (cx.engine, cx.this);
        let completed = cx.with::<Camera, _>(|c| c.run.completed.get()).ok()?;
        if !completed {
            return Some(on);
        }
        if !on {
            halt(e, me);
            return Some(false);
        }
        let capturing = cx.with::<Camera, _>(|c| c.run.capture.is_some()).ok()?;
        Some(capturing || begin(e, me))
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        self.run.shared.current.store(0, Ordering::Release);
    }
}

fn chosen(e: &Engine, me: ObjectId) -> Option<CameraInfo> {
    let device = e
        .untrack(|| e.get(me, Camera::CAMERA_DEVICE))
        .ok()
        .flatten()?;
    e.with::<CameraDevice, _>(device, |d| d.info.clone())
        .ok()
        .flatten()
}

/// Starts capturing; whether it did. A camera that cannot says why.
fn begin(e: &Engine, me: ObjectId) -> bool {
    let Some(info) = chosen(e, me) else {
        fail_quietly(e, me, NO_CAMERA);
        return false;
    };
    if Platform::of(e).permissions().status(Permission::Camera) != PermissionStatus::Granted {
        fail(e, me, NOT_PERMITTED);
        return false;
    }
    let Ok(shared) = e.with::<Camera, _>(me, |c| Arc::clone(&c.run.shared)) else {
        return false;
    };
    let capture = NEXT.fetch_add(1, Ordering::Relaxed);
    shared.current.store(capture, Ordering::Release);
    let frames = CameraFrames {
        shared: Arc::clone(&shared),
        capture,
        camera: me,
        post: Platform::of(e).poster(),
    };
    match cameras(e).open(&info, frames) {
        Ok(running) => {
            let _ = e.with_mut::<Camera, _>(me, |c| c.run.capture = Some(running));
            clear_error(e, me);
            true
        }
        Err(why) => {
            shared.current.store(0, Ordering::Release);
            fail(e, me, &why);
            false
        }
    }
}

/// Stops capturing; whether it was.
fn halt(e: &Engine, me: ObjectId) -> bool {
    let capture = e
        .with_mut::<Camera, _>(me, |c| {
            c.run.shared.current.store(0, Ordering::Release);
            c.run.capture.take()
        })
        .ok()
        .flatten();
    // Dropped outside the borrow: a service may join its thread.
    capture.is_some()
}

fn restart(e: &Engine, me: ObjectId) {
    if !begin(e, me) {
        let _ = e.set_internal(me, Camera::ACTIVE, false);
    }
}

/// A capture's camera was lost.
fn lost(e: &Engine, me: ObjectId, capture: u64, why: &str) {
    let live = e
        .with::<Camera, _>(me, |c| {
            c.run.shared.current.load(Ordering::Acquire) == capture
        })
        .unwrap_or(false);
    if !live {
        return;
    }
    halt(e, me);
    let _ = e.set_internal(me, Camera::ACTIVE, false);
    fail(e, me, why);
}

fn set_error(e: &Engine, me: ObjectId, code: i32, why: &str) -> bool {
    let before = e.untrack(|| e.get(me, Camera::ERROR_STRING)).ok();
    let changed = e.untrack(|| e.get(me, Camera::ERROR)).ok() != Some(code)
        || before.as_ref().map(Atom::as_str) != Some(why);
    let _ = e.set_internal(me, Camera::ERROR, code);
    let _ = e.set_internal(me, Camera::ERROR_STRING, Atom::from(why));
    changed
}

fn fail_quietly(e: &Engine, me: ObjectId, why: &str) {
    set_error(e, me, error::CAMERA, why);
}

fn fail(e: &Engine, me: ObjectId, why: &str) {
    let changed = set_error(e, me, error::CAMERA, why);
    let _ = e.emit_by_name(
        me,
        "errorOccurred",
        &[Value::Int(error::CAMERA), Value::from(why)],
    );
    if changed {
        let _ = e.emit_by_name(me, "errorChanged", &[]);
    }
}

fn clear_error(e: &Engine, me: ObjectId) {
    if set_error(e, me, error::NONE, "") {
        let _ = e.emit_by_name(me, "errorChanged", &[]);
    }
}

// ------------------------------------------------------------ CaptureSession

/// `CaptureSession` (QtMultimedia): joins a camera to the output that
/// shows what it captures.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtMultimedia", name = "CaptureSession", extends = QtObject)]
pub struct CaptureSession {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, object = "Camera")]
    pub camera: Option<ObjectId>,
    #[qml(notify)]
    pub video_output: Option<ObjectId>,
}

/// The camera each session last joined to an output, and whether a
/// session's destruction is listened for: a camera that outlives its
/// session is no longer shown through it.
#[derive(Default)]
struct Sessions {
    joined: RefCell<HashMap<ObjectId, ObjectId>>,
    listening: Cell<bool>,
    /// The outputs, by session, whose sink a session waits for.
    waiting: RefCell<std::collections::HashSet<(ObjectId, ObjectId)>>,
}

fn sessions_destroyed(e: &Engine, gone: &[ObjectId]) {
    let left: Vec<(ObjectId, ObjectId)> = {
        let sessions = e.extension::<Sessions>();
        // A pair whose session or output is gone waits for nothing: the
        // output's watch went with it, and an `ObjectId` is never reused.
        sessions
            .waiting
            .borrow_mut()
            .retain(|(s, o)| !gone.contains(s) && !gone.contains(o));
        let mut joined = sessions.joined.borrow_mut();
        gone.iter()
            .filter_map(|s| Some((*s, joined.remove(s)?)))
            .collect()
    };
    for (session, camera) in left {
        preview_of(e, camera, session, None);
    }
}

#[qml_impl]
impl CaptureSession {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let sessions = e.extension::<Sessions>();
        if !sessions.listening.replace(true) {
            e.on_destroyed(sessions_destroyed);
        }
        join(e, me);
        e.watch(me, CaptureSession::CAMERA.index, move |e, _| join(e, me))?;
        e.watch(me, CaptureSession::VIDEO_OUTPUT.index, move |e, _| {
            join(e, me)
        })?;
        Ok(())
    }
}

/// Joins `camera` to `input` for `session`, or, with none, takes it
/// from the output `session` joined it to; another session's stays.
fn preview_of(e: &Engine, camera: ObjectId, session: ObjectId, input: Option<VideoFrameInput>) {
    let Ok(shared) = e.with::<Camera, _>(camera, |c| Arc::clone(&c.run.shared)) else {
        return;
    };
    let mut preview = lock(&shared.preview);
    let old = match input {
        Some(input) => preview.replace((session, input)),
        None if preview.as_ref().is_some_and(|(s, _)| *s == session) => preview.take(),
        None => None,
    };
    // The capture thread may hold a clone of the old route: what it sends
    // there, or sent and is not yet shown, goes nowhere now.
    if let Some((_, old)) = old {
        old.close();
    }
}

fn join(e: &Engine, me: ObjectId) {
    let camera = e
        .untrack(|| e.get(me, CaptureSession::CAMERA))
        .ok()
        .flatten();
    let output = e
        .untrack(|| e.get(me, CaptureSession::VIDEO_OUTPUT))
        .ok()
        .flatten();
    let before = {
        let sessions = e.extension::<Sessions>();
        let mut joined = sessions.joined.borrow_mut();
        match camera {
            Some(c) => joined.insert(me, c),
            None => joined.remove(&me),
        }
    };
    if let Some(old) = before.filter(|old| Some(*old) != camera) {
        preview_of(e, old, me, None);
    }
    if let Some(camera) = camera {
        let input = output.and_then(|o| VideoFrameInput::new(e, o).ok());
        if input.is_none()
            && let Some(o) = output
        {
            wait_for_sink(e, me, o);
        }
        preview_of(e, camera, me, input);
    }
}

/// A `VideoOutput` makes its sink as it completes, which may be after a
/// session declared before it joined it: the session joins again then.
fn wait_for_sink(e: &Engine, me: ObjectId, output: ObjectId) {
    if e.with::<crate::video::VideoOutput, _>(output, |_| ())
        .is_err()
    {
        return;
    }
    let sessions = e.extension::<Sessions>();
    if !sessions.waiting.borrow_mut().insert((me, output)) {
        return;
    }
    let _ = e.watch(
        output,
        crate::video::VideoOutput::VIDEO_SINK.index,
        move |e, _| {
            // A session that went on to another output since waits for
            // this one no more.
            let still = e.untrack(|| e.get(me, CaptureSession::VIDEO_OUTPUT));
            if e.is_alive(me) && matches!(still, Ok(Some(o)) if o == output) {
                join(e, me);
            }
        },
    );
}

// ------------------------------------------------------------ FakeCameras

/// Cameras a test gives the system, whose pictures it captures itself
/// ([`FakeCameras::capture`]) on the Engine thread. Clones are the same
/// cameras.
#[derive(Clone, Default)]
pub struct FakeCameras(Rc<FakeState>);

#[derive(Default)]
struct FakeState {
    cameras: RefCell<Vec<CameraInfo>>,
    open: RefCell<Vec<(u64, String, CameraFrames)>>,
    next: Cell<u64>,
}

impl std::fmt::Debug for FakeCameras {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeCameras")
            .field("cameras", &self.0.cameras.borrow().len())
            .field("open", &self.0.open.borrow().len())
            .finish()
    }
}

struct FakeCapture(Weak<FakeState>, u64);

impl Drop for FakeCapture {
    fn drop(&mut self) {
        if let Some(state) = self.0.upgrade() {
            state.open.borrow_mut().retain(|(n, ..)| *n != self.1);
        }
    }
}

impl FakeCameras {
    /// The system has a camera called `name`; the first is the default.
    pub fn add(&self, name: &str) {
        let mut cameras = self.0.cameras.borrow_mut();
        let is_default = cameras.is_empty();
        cameras.push(CameraInfo {
            id: name.to_owned(),
            description: name.to_owned(),
            position: CameraPosition::Unspecified,
            is_default,
        });
    }

    /// The camera `name` captures `frame`; a camera nobody captures from
    /// gives it to nobody. Whether there is such a camera.
    pub fn capture(&self, name: &str, frame: &VideoFrame) -> bool {
        let found = self.0.cameras.borrow().iter().any(|c| c.id == name);
        let open: Vec<CameraFrames> = self
            .0
            .open
            .borrow()
            .iter()
            .filter(|(_, n, _)| n == name)
            .map(|(.., f)| f.clone())
            .collect();
        for frames in open {
            frames.frame(frame.clone());
        }
        found
    }

    /// The camera `name` is unplugged: it is no longer listed, and its
    /// captures stop; the first left is the default, as a system makes
    /// one. Whether there was such a camera.
    pub fn disconnect(&self, name: &str) -> bool {
        let mut cameras = self.0.cameras.borrow_mut();
        let before = cameras.len();
        cameras.retain(|c| c.id != name);
        let found = cameras.len() != before;
        if !cameras.iter().any(|c| c.is_default)
            && let Some(first) = cameras.first_mut()
        {
            first.is_default = true;
        }
        drop(cameras);
        // Out of `open` now, so the camera captures nothing more while
        // the Engine is still to hear it is gone.
        let open: Vec<_> = self
            .0
            .open
            .borrow_mut()
            .extract_if(.., |(_, n, _)| n == name)
            .collect();
        for (.., frames) in open {
            frames.fail("The camera was disconnected");
        }
        found
    }

    /// How many captures are running.
    pub fn capturing(&self) -> usize {
        self.0.open.borrow().len()
    }
}

impl CameraService for FakeCameras {
    fn cameras(&self) -> Vec<CameraInfo> {
        self.0.cameras.borrow().clone()
    }

    fn open(&self, camera: &CameraInfo, frames: CameraFrames) -> Result<Capture, String> {
        if !self.0.cameras.borrow().iter().any(|c| c.id == camera.id) {
            return Err(NO_CAMERA.to_owned());
        }
        let n = self.0.next.get() + 1;
        self.0.next.set(n);
        self.0
            .open
            .borrow_mut()
            .push((n, camera.id.clone(), frames));
        Ok(Capture::new(FakeCapture(Rc::downgrade(&self.0), n)))
    }
}
