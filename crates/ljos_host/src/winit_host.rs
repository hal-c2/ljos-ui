//! [`WinitHost`]: the host for the desktop, on winit.
//!
//! The loop waits ([`ControlFlow::Wait`] or `WaitUntil`); it is never set
//! to poll. A window redraws when the application asks for it
//! ([`HostControl::request_redraw`]) or the platform does.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{
    ElementState, Ime, MouseButton, MouseScrollDelta, StartCause, TouchPhase as WinitTouchPhase,
    WindowEvent as Raw,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, PhysicalKey};
use winit::monitor::MonitorHandle;
use winit::window::{Fullscreen, ResizeDirection, Theme, UserAttentionType, Window};

use crate::event::{
    AppState, ButtonState, Event, Key, KeyEvent, Modifiers, PointerButton, PointerEvent,
    PointerKind, TextEvent, TouchEvent, TouchPhase, WheelDelta, WindowEvent,
};
use crate::schedule::{Schedule, Sleep};
use crate::window::{
    Capabilities, Screen, WindowAttributes, WindowChange, WindowId, WindowLevel, WindowState,
};
use crate::{HostApp, HostControl, HostError, PlatformHost, Wake, Waker};

/// The desktop host. `U` is what other threads post into the loop; it is
/// made from [`Wake`] so that the host's waker can post one.
pub struct WinitHost<U: 'static> {
    event_loop: EventLoop<U>,
}

impl<U: 'static> std::fmt::Debug for WinitHost<U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WinitHost").finish_non_exhaustive()
    }
}

impl<U: From<Wake> + Send + 'static> WinitHost<U> {
    /// Connects to the display. Call it on the main thread: some platforms
    /// allow the loop nowhere else.
    ///
    /// # Errors
    /// There is no display, or the platform refused.
    pub fn new() -> Result<Self, HostError> {
        let event_loop = EventLoop::<U>::with_user_event()
            .build()
            .map_err(|e| HostError::Loop(e.to_string()))?;
        Ok(Self { event_loop })
    }

    /// Posts messages into the loop from any thread. A UI layer that has
    /// winit integration of its own (an accessibility adapter) takes this.
    pub fn proxy(&self) -> EventLoopProxy<U> {
        self.event_loop.create_proxy()
    }

    /// The display, for what is set up on it before the loop runs (a
    /// graphics instance, on another thread).
    pub fn display_handle(&self) -> winit::event_loop::OwnedDisplayHandle {
        self.event_loop.owned_display_handle()
    }
}

impl<U: From<Wake> + Send + 'static> PlatformHost for WinitHost<U> {
    type Raw = Raw;
    type User = U;
    type Control<'a> = WinitControl<'a, U>;

    fn capabilities(&self) -> Capabilities {
        capabilities(platform::loop_is_wayland(&self.event_loop))
    }

    fn waker(&self) -> Waker {
        let proxy = self.event_loop.create_proxy();
        // A send fails only once the loop has ended, when there is nothing
        // left to wake.
        Waker::new(move || drop(proxy.send_event(U::from(Wake))))
    }

    fn run<A: HostApp<Self>>(self, app: A) -> Result<(), HostError> {
        let proxy = self.event_loop.create_proxy();
        // Until the application asks for a time, the loop sleeps.
        self.event_loop.set_control_flow(ControlFlow::Wait);
        let mut runner = Runner {
            app,
            state: State::default(),
            proxy,
        };
        self.event_loop
            .run_app(&mut runner)
            .map_err(|e| HostError::Loop(e.to_string()))
    }
}

fn capabilities(wayland: bool) -> Capabilities {
    Capabilities {
        multi_window: true,
        window_position: !wayland,
        activation: true,
        window_levels: true,
        window_kinds: platform::HAS_WINDOW_KINDS && !wayland,
        modal_windows: false,
        ime: true,
        touch: true,
        insets: false,
        back: false,
    }
}

/// The windows of a running loop and what it wakes for.
#[derive(Default)]
struct State {
    windows: BTreeMap<WindowId, Arc<Window>>,
    ids: HashMap<winit::window::WindowId, WindowId>,
    next_id: u64,
    schedule: Schedule,
    exit: bool,
    /// Windows that have the keyboard: the application is active while
    /// there is one.
    focused: BTreeSet<WindowId>,
    app_state: Option<AppState>,
    /// Where the pointer was last seen in each window.
    pointer: HashMap<WindowId, [f64; 2]>,
}

/// What the application holds while it handles an event.
pub struct WinitControl<'a, U: 'static> {
    event_loop: &'a ActiveEventLoop,
    state: &'a mut State,
    proxy: &'a EventLoopProxy<U>,
}

impl<U: 'static> std::fmt::Debug for WinitControl<'_, U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WinitControl")
            .field("windows", &self.state.windows.len())
            .finish_non_exhaustive()
    }
}

impl<U: 'static> WinitControl<'_, U> {
    /// The winit window, for a UI layer that draws into it.
    pub fn window(&self, id: WindowId) -> Option<&Arc<Window>> {
        self.state.windows.get(&id)
    }

    /// The running loop, for what only winit offers.
    pub fn event_loop(&self) -> &ActiveEventLoop {
        self.event_loop
    }

    pub fn proxy(&self) -> &EventLoopProxy<U> {
        self.proxy
    }

    /// The windows open now, oldest first.
    pub fn window_ids(&self) -> Vec<WindowId> {
        self.state.windows.keys().copied().collect()
    }
}

fn logical(size: [f64; 2]) -> LogicalSize<f64> {
    LogicalSize::new(size[0], size[1])
}

fn fullscreen(state: WindowState) -> Option<Fullscreen> {
    (state == WindowState::FullScreen).then_some(Fullscreen::Borderless(None))
}

fn level(level: WindowLevel) -> winit::window::WindowLevel {
    match level {
        WindowLevel::Normal => winit::window::WindowLevel::Normal,
        WindowLevel::AlwaysOnTop => winit::window::WindowLevel::AlwaysOnTop,
        WindowLevel::AlwaysOnBottom => winit::window::WindowLevel::AlwaysOnBottom,
    }
}

fn screen(m: &MonitorHandle) -> Screen {
    let size = m.size();
    let pos = m.position();
    Screen {
        name: m.name().unwrap_or_default(),
        size: [size.width, size.height],
        position: [pos.x, pos.y],
        scale: m.scale_factor(),
        refresh_hz: m
            .refresh_rate_millihertz()
            .filter(|mhz| *mhz > 0)
            .map(|mhz| f64::from(mhz) / 1000.0),
    }
}

impl<U: 'static> HostControl for WinitControl<'_, U> {
    fn capabilities(&self) -> Capabilities {
        capabilities(platform::is_wayland(self.event_loop))
    }

    fn create_window(&mut self, a: &WindowAttributes) -> Result<WindowId, HostError> {
        let mut attrs = Window::default_attributes()
            .with_title(a.title.clone())
            .with_visible(a.visible)
            .with_decorations(a.decorations)
            .with_resizable(a.resizable)
            .with_transparent(a.transparent)
            .with_window_level(level(a.level))
            .with_active(a.active)
            .with_maximized(a.state == WindowState::Maximized)
            .with_fullscreen(fullscreen(a.state));
        if let Some(size) = a.size {
            attrs = attrs.with_inner_size(logical(size));
        }
        if let Some(size) = a.min_size {
            attrs = attrs.with_min_inner_size(logical(size));
        }
        if let Some(size) = a.max_size {
            attrs = attrs.with_max_inner_size(logical(size));
        }
        if let Some([x, y]) = a.position {
            attrs = attrs.with_position(LogicalPosition::new(x, y));
        }
        attrs = platform::with_kind(attrs, a.kind);
        if let Some(id) = &a.app_id {
            attrs = platform::with_app_id(attrs, id);
        }
        let window = self
            .event_loop
            .create_window(attrs)
            .map_err(|e| HostError::Window(e.to_string()))?;
        if a.state == WindowState::Minimized {
            window.set_minimized(true);
        }
        let id = WindowId(self.state.next_id);
        self.state.next_id += 1;
        self.state.ids.insert(window.id(), id);
        self.state.windows.insert(id, Arc::new(window));
        Ok(id)
    }

    fn destroy_window(&mut self, id: WindowId) {
        if let Some(window) = self.state.windows.remove(&id) {
            self.state.ids.remove(&window.id());
            self.state.focused.remove(&id);
            self.state.pointer.remove(&id);
        }
    }

    fn change_window(&mut self, id: WindowId, change: WindowChange) {
        let Some(w) = self.state.windows.get(&id) else {
            return;
        };
        match change {
            WindowChange::Title(t) => w.set_title(&t),
            WindowChange::Size(size) => {
                // The platform answers with a resize event when it agrees.
                let _ = w.request_inner_size(logical(size));
            }
            WindowChange::Position([x, y]) => w.set_outer_position(LogicalPosition::new(x, y)),
            WindowChange::MinSize(size) => w.set_min_inner_size(size.map(logical)),
            WindowChange::MaxSize(size) => w.set_max_inner_size(size.map(logical)),
            WindowChange::Visible(v) => w.set_visible(v),
            WindowChange::State(state) => {
                w.set_fullscreen(fullscreen(state));
                match state {
                    WindowState::Minimized => w.set_minimized(true),
                    WindowState::Maximized => {
                        w.set_minimized(false);
                        w.set_maximized(true);
                    }
                    WindowState::Normal => {
                        w.set_minimized(false);
                        w.set_maximized(false);
                    }
                    WindowState::FullScreen => w.set_minimized(false),
                }
            }
            WindowChange::Decorations(on) => w.set_decorations(on),
            WindowChange::Resizable(on) => w.set_resizable(on),
            WindowChange::Level(l) => w.set_window_level(level(l)),
            WindowChange::Activate => w.focus_window(),
            WindowChange::Alert => w.request_user_attention(Some(UserAttentionType::Informational)),
            // A platform that cannot says so here; the application was
            // answered already, as Qt's methods answer before the move.
            WindowChange::StartMove => {
                let _ = w.drag_window();
            }
            WindowChange::StartResize(edge) => {
                let _ = w.drag_resize_window(resize_direction(edge));
            }
        }
    }

    fn request_redraw(&mut self, id: WindowId) {
        if let Some(w) = self.state.windows.get(&id) {
            w.request_redraw();
        }
    }

    fn wake_at(&mut self, deadline: Option<Instant>) {
        self.state.schedule.set(deadline);
    }

    fn screens(&self) -> Vec<Screen> {
        let primary = self.event_loop.primary_monitor();
        let mut all: Vec<Screen> = primary.iter().map(screen).collect();
        all.extend(
            self.event_loop
                .available_monitors()
                .filter(|m| Some(m) != primary.as_ref())
                .map(|m| screen(&m)),
        );
        all
    }

    fn screen_of(&self, id: WindowId) -> Option<Screen> {
        let m = self.state.windows.get(&id)?.current_monitor()?;
        Some(screen(&m))
    }

    fn exit(&mut self) {
        self.state.exit = true;
    }
}

/// winit's handler around the application.
struct Runner<A, U: 'static> {
    app: A,
    state: State,
    proxy: EventLoopProxy<U>,
}

impl<A, U> Runner<A, U>
where
    U: From<Wake> + Send + 'static,
    A: HostApp<WinitHost<U>>,
{
    fn deliver(&mut self, event_loop: &ActiveEventLoop, event: Event<'_, Raw, U>) {
        let mut control = WinitControl {
            event_loop,
            state: &mut self.state,
            proxy: &self.proxy,
        };
        self.app.event(&mut control, event);
        self.settle(event_loop);
    }

    /// Applies what the application asked for: the end, or how to wait.
    fn settle(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.exit {
            event_loop.exit();
            return;
        }
        event_loop.set_control_flow(match self.state.schedule.sleep() {
            Sleep::Forever => ControlFlow::Wait,
            Sleep::Until(at) => ControlFlow::WaitUntil(at),
        });
    }

    /// Tells the application when it gained or lost its user.
    fn report_app_state(&mut self, event_loop: &ActiveEventLoop, state: AppState) {
        if self.state.app_state != Some(state) {
            self.state.app_state = Some(state);
            self.deliver(event_loop, Event::AppState(state));
        }
    }
}

impl<A, U> ApplicationHandler<U> for Runner<A, U>
where
    U: From<Wake> + Send + 'static,
    A: HostApp<WinitHost<U>>,
{
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        // Asked of the clock, not of the cause: a platform event that
        // arrives just after the deadline wakes the loop with its own
        // cause, and the deadline has come all the same.
        let due = self.state.schedule.take_due(Instant::now());
        if due || matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.state.schedule.set(None);
            self.deliver(event_loop, Event::Wake);
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.deliver(event_loop, Event::SurfaceCreated);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.deliver(event_loop, Event::SurfaceDestroyed);
        self.report_app_state(event_loop, AppState::Suspended);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: U) {
        self.deliver(event_loop, Event::User(event));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        raw: Raw,
    ) {
        let Some(&id) = self.state.ids.get(&window_id) else {
            return;
        };
        let Some(window) = self.state.windows.get(&id).cloned() else {
            return;
        };
        match &raw {
            Raw::Focused(true) => {
                self.state.focused.insert(id);
            }
            Raw::Focused(false) => {
                self.state.focused.remove(&id);
            }
            Raw::CursorMoved { position, .. } => {
                self.state.pointer.insert(id, [position.x, position.y]);
            }
            _ => {}
        }
        let pointer = self.state.pointer.get(&id).copied();
        let event = translate(&raw, &window, pointer);
        if matches!(raw, Raw::CursorLeft { .. }) {
            self.state.pointer.remove(&id);
        }
        self.deliver(
            event_loop,
            Event::Window {
                id,
                event,
                raw: Some(&raw),
            },
        );
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Here and not at each focus event: when the keyboard goes from
        // one window of the application to another, the first loses it
        // before the second has it, and the application was never left.
        if self.state.app_state != Some(AppState::Suspended) && !self.state.windows.is_empty() {
            let state = if self.state.focused.is_empty() {
                AppState::Inactive
            } else {
                AppState::Active
            };
            self.report_app_state(event_loop, state);
        }
        self.deliver(event_loop, Event::AboutToWait);
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.deliver(event_loop, Event::MemoryPressure);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.deliver(event_loop, Event::Exiting);
        // The windows go with the loop they belong to.
        self.state.windows.clear();
        self.state.ids.clear();
    }
}

fn button_state(state: ElementState) -> ButtonState {
    match state {
        ElementState::Pressed => ButtonState::Pressed,
        ElementState::Released => ButtonState::Released,
    }
}

fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Back,
        MouseButton::Forward => PointerButton::Forward,
        MouseButton::Other(n) => PointerButton::Other(n),
    }
}

fn key(key: &WinitKey) -> Key {
    match key {
        // winit names its keys as the UI Events specification does.
        WinitKey::Named(named) => Key::Named(format!("{named:?}")),
        WinitKey::Character(c) => Key::Character(c.to_string()),
        WinitKey::Unidentified(_) | WinitKey::Dead(_) => Key::Unidentified,
    }
}

/// The portable form of one of winit's window events.
fn translate(raw: &Raw, window: &Window, pointer: Option<[f64; 2]>) -> WindowEvent {
    let at = |kind| WindowEvent::Pointer(PointerEvent { kind, pos: pointer });
    match raw {
        Raw::Resized(size) => WindowEvent::Resized {
            px: [size.width, size.height],
            scale: window.scale_factor(),
        },
        Raw::ScaleFactorChanged { scale_factor, .. } => {
            let size = window.inner_size();
            WindowEvent::Resized {
                px: [size.width, size.height],
                scale: *scale_factor,
            }
        }
        Raw::Moved(pos) => WindowEvent::Moved { x: pos.x, y: pos.y },
        Raw::Focused(on) => WindowEvent::Focus(*on),
        Raw::CloseRequested => WindowEvent::CloseRequested,
        Raw::RedrawRequested => WindowEvent::RedrawRequested,
        Raw::Occluded(on) => WindowEvent::Occluded(*on),
        Raw::ThemeChanged(theme) => WindowEvent::Theme {
            dark: *theme == Theme::Dark,
        },
        Raw::Destroyed => WindowEvent::Destroyed,
        Raw::CursorMoved { .. } => at(PointerKind::Moved),
        Raw::CursorEntered { .. } => at(PointerKind::Entered),
        Raw::CursorLeft { .. } => at(PointerKind::Left),
        Raw::MouseInput { state, button, .. } => at(PointerKind::Button(
            pointer_button(*button),
            button_state(*state),
        )),
        Raw::MouseWheel { delta, .. } => at(PointerKind::Wheel(match delta {
            MouseScrollDelta::LineDelta(x, y) => WheelDelta::Lines([f64::from(*x), f64::from(*y)]),
            MouseScrollDelta::PixelDelta(p) => WheelDelta::Pixels([p.x, p.y]),
        })),
        Raw::Touch(touch) => WindowEvent::Touch(TouchEvent {
            id: touch.id,
            phase: match touch.phase {
                WinitTouchPhase::Started => TouchPhase::Started,
                WinitTouchPhase::Moved => TouchPhase::Moved,
                WinitTouchPhase::Ended => TouchPhase::Ended,
                WinitTouchPhase::Cancelled => TouchPhase::Cancelled,
            },
            pos: [touch.location.x, touch.location.y],
            force: touch.force.map(|f| f.normalized()),
        }),
        Raw::KeyboardInput { event, .. } => WindowEvent::Key(KeyEvent {
            key: key(&event.logical_key),
            code: match event.physical_key {
                PhysicalKey::Code(code) => Some(format!("{code:?}")),
                PhysicalKey::Unidentified(_) => None,
            },
            state: button_state(event.state),
            repeat: event.repeat,
            text: event.text.as_ref().map(ToString::to_string),
        }),
        Raw::ModifiersChanged(m) => {
            let m = m.state();
            WindowEvent::Modifiers(Modifiers {
                shift: m.shift_key(),
                control: m.control_key(),
                alt: m.alt_key(),
                meta: m.super_key(),
            })
        }
        Raw::Ime(ime) => WindowEvent::Text(match ime {
            Ime::Enabled => TextEvent::Enabled(true),
            Ime::Disabled => TextEvent::Enabled(false),
            Ime::Preedit(text, selection) => TextEvent::Composing {
                text: text.clone(),
                selection: *selection,
            },
            Ime::Commit(text) => TextEvent::Commit(text.clone()),
        }),
        _ => WindowEvent::Other,
    }
}

/// What differs between window systems.
mod platform {
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::window::WindowAttributes;

    use crate::window::WindowKind;

    #[cfg(all(
        unix,
        not(target_vendor = "apple"),
        not(target_os = "android"),
        not(target_os = "emscripten"),
        not(target_os = "redox")
    ))]
    mod imp {
        use winit::platform::wayland::{ActiveEventLoopExtWayland, EventLoopExtWayland};
        use winit::platform::x11::{WindowAttributesExtX11, WindowType};

        use super::*;

        /// X11 knows kinds of window; Wayland has no word for them.
        pub(crate) const HAS_WINDOW_KINDS: bool = true;

        pub(crate) fn is_wayland(event_loop: &ActiveEventLoop) -> bool {
            event_loop.is_wayland()
        }

        pub(crate) fn loop_is_wayland<U: 'static>(event_loop: &EventLoop<U>) -> bool {
            event_loop.is_wayland()
        }

        pub(crate) fn with_kind(attrs: WindowAttributes, kind: WindowKind) -> WindowAttributes {
            let kind = match kind {
                // Left alone: the window manager's default is this.
                WindowKind::Normal => return attrs,
                WindowKind::Dialog => WindowType::Dialog,
                WindowKind::Tool => WindowType::Utility,
                WindowKind::Popup => WindowType::PopupMenu,
                WindowKind::ToolTip => WindowType::Tooltip,
                WindowKind::Splash => WindowType::Splash,
            };
            attrs.with_x11_window_type(vec![kind])
        }

        pub(crate) fn with_app_id(attrs: WindowAttributes, id: &str) -> WindowAttributes {
            // Wayland's app id and X11's class are one setting in winit;
            // both of its traits have this method, hence by name.
            winit::platform::wayland::WindowAttributesExtWayland::with_name(attrs, id, id)
        }
    }

    #[cfg(not(all(
        unix,
        not(target_vendor = "apple"),
        not(target_os = "android"),
        not(target_os = "emscripten"),
        not(target_os = "redox")
    )))]
    mod imp {
        use super::*;

        pub(crate) const HAS_WINDOW_KINDS: bool = false;

        pub(crate) fn is_wayland(_: &ActiveEventLoop) -> bool {
            false
        }

        pub(crate) fn loop_is_wayland<U: 'static>(_: &EventLoop<U>) -> bool {
            false
        }

        pub(crate) fn with_kind(attrs: WindowAttributes, _: WindowKind) -> WindowAttributes {
            attrs
        }

        pub(crate) fn with_app_id(attrs: WindowAttributes, _: &str) -> WindowAttributes {
            attrs
        }
    }

    pub(crate) use imp::{HAS_WINDOW_KINDS, is_wayland, loop_is_wayland, with_app_id, with_kind};
}

fn resize_direction(edge: crate::ResizeEdge) -> ResizeDirection {
    use crate::ResizeEdge as E;
    match edge {
        E::North => ResizeDirection::North,
        E::NorthEast => ResizeDirection::NorthEast,
        E::East => ResizeDirection::East,
        E::SouthEast => ResizeDirection::SouthEast,
        E::South => ResizeDirection::South,
        E::SouthWest => ResizeDirection::SouthWest,
        E::West => ResizeDirection::West,
        E::NorthWest => ResizeDirection::NorthWest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_has_no_window_positions_and_no_window_kinds() {
        let c = capabilities(true);
        assert!(!c.window_position);
        assert!(!c.window_kinds);
        assert!(c.multi_window && c.ime);
    }

    #[test]
    fn no_desktop_host_can_make_a_window_modal() {
        assert!(!capabilities(false).modal_windows);
        assert!(!capabilities(true).modal_windows);
    }

    #[test]
    fn only_a_full_screen_window_is_given_the_screen() {
        assert!(fullscreen(WindowState::FullScreen).is_some());
        for s in [
            WindowState::Normal,
            WindowState::Minimized,
            WindowState::Maximized,
        ] {
            assert!(fullscreen(s).is_none());
        }
    }

    #[test]
    fn keys_keep_the_names_of_the_ui_events_specification() {
        use winit::keyboard::NamedKey;
        assert_eq!(
            key(&WinitKey::Named(NamedKey::ArrowLeft)),
            Key::Named("ArrowLeft".to_owned())
        );
        assert_eq!(
            key(&WinitKey::Character("a".into())),
            Key::Character("a".to_owned())
        );
    }

    #[test]
    fn buttons_are_named_by_what_they_do() {
        assert_eq!(pointer_button(MouseButton::Left), PointerButton::Primary);
        assert_eq!(pointer_button(MouseButton::Right), PointerButton::Secondary);
        assert_eq!(
            pointer_button(MouseButton::Other(7)),
            PointerButton::Other(7)
        );
        assert_eq!(button_state(ElementState::Pressed), ButtonState::Pressed);
    }
}
