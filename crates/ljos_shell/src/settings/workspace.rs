//! `NSWorkspace`'s accessibility display options (`crate::settings`).

use std::ptr::NonNull;

use block2::RcBlock;
use ljos_a11y::{Contrast, HostPrefs};
use ljos_platform::Post;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSWorkspace, NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification};
use objc2_foundation::{NSNotification, NSObjectProtocol, NSOperationQueue};

/// The observer of the workspace's notification, taken off when the
/// engine goes.
pub(crate) struct Watch {
    observer: Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

fn read() -> HostPrefs {
    let workspace = NSWorkspace::sharedWorkspace();
    HostPrefs {
        reduced_motion: Some(workspace.accessibilityDisplayShouldReduceMotion()),
        contrast: Some(if workspace.accessibilityDisplayShouldIncreaseContrast() {
            Contrast::High
        } else {
            Contrast::NoPreference
        }),
        ..HostPrefs::default()
    }
}

pub(crate) fn watch(post: Post) -> Watch {
    super::report(&post, read());
    let block = RcBlock::new(move |_: NonNull<NSNotification>| {
        super::report(&post, read());
    });
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: the notification has no object to type, the main queue
    // is the Engine thread's (the shell runs it on the main thread on
    // macOS), and the block only sends a message.
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &block,
        )
    };
    Watch {
        observer: Some(observer),
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if let Some(observer) = self.observer.take() {
            // SAFETY: the observer is the one the centre gave.
            unsafe {
                NSWorkspace::sharedWorkspace()
                    .notificationCenter()
                    .removeObserver(observer.as_ref());
            }
        }
    }
}
