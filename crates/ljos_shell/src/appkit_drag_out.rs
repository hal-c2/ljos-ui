//! Drags out of the application on macOS, which winit 0.30 has no way to
//! start: an `NSDraggingSession` begun on the content view of the window
//! the press was in, with a dragging source of the shell's.
//!
//! The session is not modal: `beginDraggingSessionWithItems:event:source:`
//! returns at once and AppKit runs the drag from the main run loop, which
//! winit's loop is, so the application keeps running while it is out, and
//! a drag back over its own window reaches the destination's methods
//! `crate::appkit_drag` gave the view, as one from another application
//! does. AppKit tells the source how it ended
//! (`draggingSession:endedAtPoint:operation:`), which posts
//! [`DragStep::OutEnded`] for the loop to settle.
//!
//! AppKit starts a session from a mouse event. The Engine asks for the
//! drag in a turn of the loop that need not be the one that heard the
//! press or the move, so the event is the application's current one when
//! it is a mouse press or drag, and otherwise a drag event made at the
//! pointer in the key window. DECISION: a made event over keeping the
//! last press the view saw, which would mean replacing winit's
//! `mouseDown:`; the event only says where the drag starts.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use ljos_platform::drop_systems::appkit;
use ljos_platform::{DragActions, DragData, DropAction};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession,
    NSDraggingSource, NSEvent, NSEventModifierFlags, NSEventType, NSPasteboardItem,
};
use objc2_foundation::{NSArray, NSPoint, NSProcessInfo, NSRect, NSSize, NSString};

use crate::drags::DragStep;

/// The size the drag's picture is shown at, in points.
const PICTURE: f64 = 32.0;

/// How a drag out ended, until the loop asks.
type Ended = Rc<Cell<Option<Option<DropAction>>>>;

struct Ivars {
    mask: u64,
    ended: Ended,
    under_way: Rc<Cell<bool>>,
    post: Arc<dyn Fn(DragStep) + Send + Sync>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the class
    // does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LjosShellDragSource"]
    #[ivars = Ivars]
    struct Source;

    unsafe impl NSObjectProtocol for Source {}

    unsafe impl NSDraggingSource for Source {
        /// The same within the application and out of it: the application
        /// takes its own drags as another would.
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn operations(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation(self.ivars().mask as usize)
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn ended(&self, _session: &NSDraggingSession, _at: NSPoint, operation: NSDragOperation) {
            let ivars = self.ivars();
            ivars.under_way.set(false);
            ivars.ended.set(Some(appkit::action_of(operation.0 as u64)));
            (ivars.post)(DragStep::OutEnded);
        }
    }
);

/// The application's drags out of its windows, on the main thread.
pub(crate) struct Out {
    post: Arc<dyn Fn(DragStep) + Send + Sync>,
    ended: Ended,
    under_way: Rc<Cell<bool>>,
    /// The source of the last drag, which the session does not keep.
    source: Option<Retained<Source>>,
}

impl Out {
    pub(crate) fn new(post: Arc<dyn Fn(DragStep) + Send + Sync>) -> Self {
        Self {
            post,
            ended: Rc::new(Cell::new(None)),
            under_way: Rc::new(Cell::new(false)),
            source: None,
        }
    }

    /// Begins a session carrying `data`. `false`: one is under way, it
    /// carries nothing, or there is no window to start it from.
    pub(crate) fn start(&mut self, data: &DragData, actions: DragActions) -> bool {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        if self.under_way.get() {
            return false;
        }
        let items = appkit::items(data);
        if items.is_empty() {
            return false;
        }
        let app = NSApplication::sharedApplication(mtm);
        let pressed = app.currentEvent().filter(|e| {
            [
                NSEventType::LeftMouseDown,
                NSEventType::LeftMouseDragged,
                NSEventType::RightMouseDown,
                NSEventType::RightMouseDragged,
                NSEventType::OtherMouseDown,
                NSEventType::OtherMouseDragged,
            ]
            .contains(&e.r#type())
        });
        let Some(window) = pressed
            .as_ref()
            .and_then(|e| e.window(mtm))
            .or_else(|| app.keyWindow())
            .or_else(|| app.mainWindow())
        else {
            return false;
        };
        let Some(view) = window.contentView() else {
            return false;
        };
        let event = match pressed.filter(|e| e.window(mtm).as_deref() == Some(&*window)) {
            Some(event) => event,
            None => {
                let made = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
                    NSEventType::LeftMouseDragged,
                    window.mouseLocationOutsideOfEventStream(),
                    NSEventModifierFlags::empty(),
                    NSProcessInfo::processInfo().systemUptime(),
                    window.windowNumber(),
                    None,
                    0,
                    1,
                    1.0,
                );
                let Some(made) = made else {
                    return false;
                };
                made
            }
        };
        let at = view.convertPoint_fromView(event.locationInWindow(), None);
        let frame = NSRect::new(
            NSPoint::new(at.x - PICTURE / 2.0, at.y - PICTURE / 2.0),
            NSSize::new(PICTURE, PICTURE),
        );
        // DECISION: the application's icon as the drag's picture, as
        // `Drag.imageSource` is not drawn yet; AppKit shows the frame it is
        // given and nothing when there is no picture.
        let picture = app.applicationIconImage();
        let dragged: Vec<Retained<NSDraggingItem>> = items
            .iter()
            .map(|kinds| {
                let item = NSPasteboardItem::new();
                for (kind, text) in kinds {
                    item.setString_forType(&NSString::from_str(text), &NSString::from_str(kind));
                }
                let dragged = NSDraggingItem::initWithPasteboardWriter(
                    NSDraggingItem::alloc(),
                    ProtocolObject::from_ref(&*item),
                );
                let contents: Option<&AnyObject> = picture.as_deref().map(|p| p.as_ref());
                // SAFETY: the contents are an image, which the frame
                // shows.
                unsafe { dragged.setDraggingFrame_contents(frame, contents) };
                dragged
            })
            .collect();
        let source = mtm.alloc::<Source>().set_ivars(Ivars {
            mask: appkit::mask_of(actions.allowed),
            ended: Rc::clone(&self.ended),
            under_way: Rc::clone(&self.under_way),
            post: Arc::clone(&self.post),
        });
        // SAFETY: NSObject's `init`, on an object just allocated.
        let source: Retained<Source> = unsafe { msg_send![super(source), init] };
        self.ended.set(None);
        self.under_way.set(true);
        let _session = view.beginDraggingSessionWithItems_event_source(
            &NSArray::from_retained_slice(&dragged),
            &event,
            ProtocolObject::from_ref(&*source),
        );
        self.source = Some(source);
        true
    }

    /// AppKit has no way to end a session early: the drag goes on until
    /// the user lets go, and how it ended is not told, as the application
    /// gave it up.
    pub(crate) fn cancel(&mut self) {
        self.ended.set(None);
    }

    pub(crate) fn ended(&mut self) -> Option<Option<DropAction>> {
        let ended = self.ended.take();
        if ended.is_some() {
            self.source = None;
        }
        ended
    }
}
