//! [`crate::run_headless`]: the application runs as [`crate::run`] runs
//! it, with no window and no display (ARCHITECTURE.md §19.7). The scene
//! is laid out at the configured size, its passes run on the real clock
//! as a window's would, and between them the thread sleeps until a timer,
//! an animation's frame or another thread's waker is due: nothing polls.
//!
//! Every QML `Window` is an item of the scene, as in a host of its own
//! (`ShellApp::new`), so that an agent's picture of the root
//! (`app_screenshot`, which the scene draws itself) is the whole
//! application. A pass's egui output is not painted: nothing is shown.
//! A host that streams the frames to another process would tessellate
//! that output where [`Headless::pass`] drops it.

use std::cell::Cell;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

#[cfg(feature = "mcp")]
use ljos_controls::WindowSystem;
use ljos_egui::ResizeMode;
use ljos_engine::{Engine, ObjectId};
use ljos_runtime::SystemClock;

use crate::desktop::{
    AfterRest, HEAP_TRIM_AFTER, HEARTBEAT, HighWater, OBJECTS_GONE_BEFORE_TRIM,
    REST_BEFORE_COLLECTING, earlier, use_fonts,
};
use crate::platform::Display;
use crate::{Hosting, Setup, SetupError, ShellApp, ShellConfig, ShellError};

thread_local! {
    /// Whether the loop's pass runs on this thread: egui asks for passes
    /// of its own during one, which the scene's own asking stands for.
    static IN_PASS: Cell<bool> = const { Cell::new(false) };
}

/// When the loop is to run its next pass. Any thread sets it; the loop
/// sleeps on it.
#[derive(Debug, Default)]
struct Alarm {
    state: Mutex<AlarmState>,
    rang: Condvar,
}

#[derive(Debug, Default)]
struct AlarmState {
    due: Option<Instant>,
    /// Times another thread set it since the loop last woke.
    posted: u32,
}

impl Alarm {
    /// Due at `at` unless due earlier already. `posted` when another
    /// thread (or egui for one) asks, not the loop for itself.
    fn set(&self, at: Instant, posted: bool) {
        let mut state = crate::lock(&self.state);
        state.posted += u32::from(posted);
        if state.due.is_none_or(|due| at < due) {
            state.due = Some(at);
            self.rang.notify_one();
        }
    }

    /// Sleeps until it is due, then forgets it: what is asked from here
    /// on is for another pass. Returns how many times other threads set
    /// it meanwhile.
    fn wait(&self) -> u32 {
        let mut state = crate::lock(&self.state);
        loop {
            let now = Instant::now();
            state = match state.due {
                Some(due) if due <= now => {
                    state.due = None;
                    return std::mem::take(&mut state.posted);
                }
                Some(due) => {
                    self.rang
                        .wait_timeout(state, due - now)
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0
                }
                None => self
                    .rang
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            };
        }
    }
}

/// [`crate::run_headless`].
pub(crate) fn run<M: Send + 'static>(
    config: ShellConfig,
    setup: impl FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError> + 'static,
) -> Result<(), ShellError> {
    #[cfg(feature = "mcp")]
    let began = Instant::now();
    let desktop_font = config
        .system_fonts
        .then(crate::desktop_font::Reading::start);
    use_fonts(&config, desktop_font);
    let alarm = Arc::new(Alarm::default());
    let ctx = new_context(&alarm);
    let hosting = Hosting {
        tree: None,
        changed: None,
        wake: {
            let alarm = Arc::clone(&alarm);
            Arc::new(move |delay| {
                if let Some(at) = Instant::now().checked_add(delay) {
                    alarm.set(at, true);
                }
            })
        },
        display: Display::None,
    };
    #[cfg_attr(not(feature = "mcp"), allow(unused_mut))]
    let mut app = ShellApp::hosted(&ctx, &config, Arc::new(SystemClock::new()), setup, hosting)?;
    #[cfg(feature = "mcp")]
    {
        app.record_startup(ljos_dev::mcp::StartupStep::Built, began.elapsed());
        // `app_resize` is answered by the pass at the size asked for,
        // within a root window's limits, as a window's is.
        app.set_resizes_window();
    }
    // Every root, a `Window` too, is as large as the run says: there is
    // no platform to say otherwise.
    app.view_mut().set_resize_mode(ResizeMode::RootToView);
    let opaque = config.background.to_rgba8()[3] == 255;
    app.set_backdrop(!opaque);
    app.view_mut().set_fold_backdrop(opaque);
    let mut headless = Headless {
        app,
        ctx,
        alarm,
        size: config.size,
        root_sized: config.root_sized,
        started: Instant::now(),
        asked: None,
        drawn_once: false,
        scripts_prepared: false,
        heap_trim: None,
        loading: false,
        objects_high: HighWater::default(),
        collect: AfterRest::default(),
        #[cfg(feature = "mcp")]
        began,
    };
    headless.run();
    Ok(())
}

/// The context of the scene's passes. A repaint asked of it from another
/// thread wakes the loop; one asked during a pass is egui's own and not
/// heard, as in the desktop loop: the scene says what it wants next
/// ([`ljos_egui::QmlView::take_asked`]).
fn new_context(alarm: &Arc<Alarm>) -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(ljos_egui::font_definitions());
    ctx.options_mut(|o| o.max_passes = NonZeroUsize::new(2).unwrap_or(NonZeroUsize::MIN));
    let alarm = Arc::clone(alarm);
    ctx.set_request_repaint_callback(move |info| {
        if IN_PASS.get() {
            return;
        }
        // A delay too long to say is "not before something else happens".
        if let Some(at) = Instant::now().checked_add(info.delay) {
            alarm.set(at, true);
        }
    });
    ctx
}

struct Headless<M> {
    app: ShellApp<M>,
    ctx: egui::Context,
    alarm: Arc<Alarm>,
    /// The scene's size in logical pixels.
    size: [f32; 2],
    /// The size is read from the root before the first pass.
    root_sized: bool,
    /// What egui is told the time is measured from.
    started: Instant,
    /// When the scene asked for its next pass.
    asked: Option<Instant>,
    drawn_once: bool,
    scripts_prepared: bool,
    /// When the memory freed by start-up or a burst is given back.
    heap_trim: Option<Instant>,
    /// Pictures were loading at the end of the last pass.
    loading: bool,
    objects_high: HighWater,
    collect: AfterRest,
    #[cfg(feature = "mcp")]
    began: Instant,
}

impl<M: Send + 'static> Headless<M> {
    /// Runs until the application quits (`Qt.quit()`). There is no last
    /// window to close.
    fn run(&mut self) {
        let engine = Rc::clone(self.app.engine());
        self.alarm.set(Instant::now(), false);
        loop {
            #[cfg_attr(not(feature = "mcp"), allow(unused_variables))]
            let posted = self.alarm.wait();
            self.pass();
            #[cfg(feature = "mcp")]
            if self.app.records_wakes() {
                self.app.record_wake(&ljos_dev::mcp::HostWake {
                    deadline: posted == 0,
                    posted,
                    passes: 1,
                    ..ljos_dev::mcp::HostWake::default()
                });
            }
            if engine.quit_requested() {
                return;
            }
            self.settle(&engine);
        }
    }

    /// One pass of the scene: what other threads and the agent brought,
    /// then a frame at the size of the run. The pictures it asked for
    /// start loading after it, as after a drawn frame.
    fn pass(&mut self) {
        let engine = Rc::clone(self.app.engine());
        if std::mem::take(&mut self.root_sized) {
            self.size = root_size(&mut self.app, &engine, self.size);
        }
        let images = ljos_quick::ImageStore::of(&engine);
        if images.has_deferred() {
            // Laid out first, so that they decode at the size shown.
            self.app.view_mut().scene_mut().lay_out(&engine);
        }
        if self.drawn_once {
            let moving = self
                .asked
                .is_some_and(|at| at <= Instant::now() + HEARTBEAT);
            images.wait_for_later_frame(&engine, moving);
        } else {
            images.wait_for_frame(&engine, ljos_quick::ImageStore::FIRST_FRAME_WAIT);
        }
        let began = Instant::now();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                self.size.into(),
            )),
            focused: false,
            time: Some(began.duration_since(self.started).as_secs_f64()),
            ..egui::RawInput::default()
        };
        let app = &mut self.app;
        // The first pass lays the scene out and starts its timers before
        // anything is asked of it: an agent that found the inspector as
        // the root was built must not click an item of no size or move a
        // clock no timer runs on yet. What waits is taken in the next pass,
        // at once.
        let first = !self.drawn_once;
        IN_PASS.set(true);
        let mut output = self.ctx.run_ui(raw, |ui| {
            if !first {
                app.begin_pass();
            }
            app.show(ui);
        });
        IN_PASS.set(false);
        // What would be painted, and the font atlas it is painted with, go
        // unused: nothing shows them. A host streaming frames takes both
        // here.
        output.textures_delta.clear();
        #[cfg(feature = "mcp")]
        if self.app.waits_for_resize() {
            // The scene has the size the agent asked for: there is no
            // platform to refuse it.
            self.app.finish_resize(true);
        }
        self.asked = self.app.view_mut().take_asked().map(|after| began + after);
        ljos_quick::ImageStore::of(&engine).frame_drawn(&engine);
        if !std::mem::replace(&mut self.drawn_once, true) {
            self.alarm.set(Instant::now(), false);
            self.heap_trim = Some(Instant::now() + HEAP_TRIM_AFTER);
            #[cfg(feature = "mcp")]
            self.app
                .record_startup(ljos_dev::mcp::StartupStep::FirstFrame, self.began.elapsed());
        }
    }

    /// The bookkeeping between passes, as the desktop loop's, and the
    /// time of the next.
    fn settle(&mut self, engine: &Engine) {
        #[cfg(feature = "mcp")]
        self.serve_resize(engine);
        // Once, after the first frame: the first click then runs its
        // handler and not the start of the JS host too.
        if self.drawn_once && !self.scripts_prepared {
            self.scripts_prepared = true;
            engine.prepare_scripts();
        }
        self.trim_heap_when_done(engine);
        let now = Instant::now();
        let due = earlier(self.asked, self.heap_trim);
        let busy = due.is_some_and(|at| at < now + REST_BEFORE_COLLECTING);
        if self.collect.settled(
            busy,
            engine.wants_idle_collect(),
            now,
            REST_BEFORE_COLLECTING,
        ) {
            engine.collect_if_idle();
        }
        let next = earlier(due, self.collect.next());
        #[cfg(feature = "mcp")]
        self.app.record_rest(
            ljos_dev::mcp::Rest {
                wake_at: next,
                // What an agent sees is the scene itself: a frame is owed
                // only while a pass is due.
                frame_owed: self.asked.is_some_and(|at| at <= now),
                pointer: None,
            },
            self.collect.next(),
        );
        if let Some(next) = next {
            self.alarm.set(next, false);
        }
    }

    /// As the desktop loop does: start-up, a burst of picture loads and a
    /// page of objects going free memory glibc keeps; a while after each,
    /// it is given back.
    fn trim_heap_when_done(&mut self, engine: &Engine) {
        let loading = ljos_quick::ImageStore::of(engine).pending() > 0;
        if std::mem::replace(&mut self.loading, loading) && !loading {
            self.heap_trim = Some(Instant::now() + HEAP_TRIM_AFTER);
        }
        let objects = engine.object_count();
        if self.objects_high.fell(objects, OBJECTS_GONE_BEFORE_TRIM) {
            self.objects_high.reset(objects);
            self.heap_trim = Some(Instant::now() + HEAP_TRIM_AFTER);
        }
        if loading {
            self.heap_trim = self.heap_trim.map(|_| Instant::now() + HEAP_TRIM_AFTER);
        }
        if self.heap_trim.is_some_and(|t| t <= Instant::now()) {
            self.heap_trim = None;
            crate::give_back_freed_heap();
        }
    }

    /// The size an agent asked for (`app_resize`), within the limits of
    /// a root that is a `Window`; the next pass is at it, and answers.
    #[cfg(feature = "mcp")]
    fn serve_resize(&mut self, engine: &Engine) {
        let Some(size) = self.app.take_resize_request() else {
            return;
        };
        let spec = self
            .app
            .view()
            .scene()
            .root()
            .and_then(|root| WindowSystem::of(engine).spec(engine, root));
        let size = match spec {
            Some(spec) => crate::window_spec::within_limits(&spec, size),
            None => size,
        };
        #[allow(clippy::cast_possible_truncation)]
        {
            self.size = [size[0] as f32, size[1] as f32];
        }
        self.alarm.set(Instant::now(), false);
    }
}

/// The root's own size, laid out first, as a Text knows its implicit size
/// only then; `or` for a side it leaves at 0. A `Window` is read as an
/// item is: its size is the one it asks the platform for.
fn root_size<M: Send + 'static>(app: &mut ShellApp<M>, engine: &Engine, or: [f32; 2]) -> [f32; 2] {
    let Some(root) = app.view().scene().root() else {
        return or;
    };
    app.view_mut().scene_mut().lay_out(engine);
    let side = |name, or| {
        let n = engine.read_by_name(root, name).to_number();
        #[allow(clippy::cast_possible_truncation)]
        if n >= 1.0 { n.ceil() as f32 } else { or }
    };
    [side("width", or[0]), side("height", or[1])]
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn the_alarm_keeps_the_earliest_time_and_counts_other_threads() {
        let alarm = Alarm::default();
        let now = Instant::now();
        alarm.set(now + Duration::from_secs(60), false);
        alarm.set(now, true);
        alarm.set(now + Duration::from_secs(30), true);
        assert_eq!(alarm.wait(), 2);
        assert_eq!(crate::lock(&alarm.state).due, None);
    }

    #[test]
    fn the_alarm_wakes_the_sleeping_loop_from_another_thread() {
        let alarm = Arc::new(Alarm::default());
        let ringer = {
            let alarm = Arc::clone(&alarm);
            std::thread::spawn(move || alarm.set(Instant::now(), true))
        };
        assert_eq!(alarm.wait(), 1);
        ringer.join().unwrap();
    }
}
