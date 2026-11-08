//! The system's menu bar on macOS: `NSApp.mainMenu`, made from the
//! [`SystemBar`] the controls describe (`ljos_controls::system_menu`).
//!
//! The application menu is AppKit's usual one, as Qt's is: About and
//! Preferences… when an item of the application's takes the place,
//! Services, Hide, Hide Others, Show All, and Quit. Quit is always there;
//! it triggers the application's item that quits, or asks every window to
//! close as its user would (`system_menu::quit`), so `onClosing` runs
//! where winit's default menu ended the process. The host is built
//! without winit's menu (`WinitHost::with_options`), which would replace
//! this one when the loop starts.
//!
//! An item AppKit sends to the menu's target is handed to the Engine
//! thread through the platform's inbox, and triggered at the next frame:
//! AppKit calls the target from inside its own event handling, where the
//! shell's loop and the Engine are not to be entered. A key equivalent is
//! taken by the menu before the window hears the key, so an action's
//! `Shortcut` does not also fire.

use ljos_platform::{
    KeyEquivalent, MenuBarService, Post, SystemBar, SystemEntry, SystemItem, SystemMenu,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSEventModifierFlags, NSMenu,
    NSMenuItem, NSWindow,
};
use objc2_foundation::{
    NSDictionary, NSInteger, NSNumber, NSObjectProtocol, NSProcessInfo, NSString, NSUserDefaults,
};

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the class
    // does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LjosShellMenuTarget"]
    #[ivars = Post]
    struct Target;

    unsafe impl NSObjectProtocol for Target {}

    impl Target {
        /// An item of the application's: its tag is its id.
        #[unsafe(method(choose:))]
        fn choose(&self, sender: &NSMenuItem) {
            let id = sender.tag() as u64;
            self.ivars().send(move |e| {
                ljos_controls::system_menu::choose(e, id);
            });
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            self.ivars().send(ljos_controls::system_menu::quit);
        }
    }
);

/// `NSApp.mainMenu`, made again whenever the controls show another bar.
pub(crate) struct AppKitMenuBar {
    target: Retained<Target>,
    mtm: MainThreadMarker,
}

impl AppKitMenuBar {
    /// Made on the main thread, and shows the application menu at once, so
    /// an application with no `MenuBar` has one. `None` off the main
    /// thread.
    pub(crate) fn new(post: Post) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let target = mtm.alloc::<Target>().set_ivars(post);
        // SAFETY: NSObject's `init` on a freshly allocated object.
        let target: Retained<Target> = unsafe { msg_send![super(target), init] };
        // AppKit adds items of its own to an application's menus: "Enter
        // Full Screen" to the one titled View (else Window), beside an
        // application's own full-screen item and on the same key, and "Show
        // Tab Bar" and "Show All Tabs" for windows it would group into tabs.
        // An application's menus hold its items alone (#201), so both are
        // turned off before the first menu is set. The first is a default
        // registered for this run only: setting it would write the
        // application's preferences file at every launch.
        let key = NSString::from_str("NSFullScreenMenuItemEverywhere");
        let off = NSNumber::numberWithBool(false);
        let defaults = NSDictionary::<NSString, AnyObject>::from_slices(&[&*key], &[&**off]);
        // SAFETY: the dictionary's keys are strings and its values property
        // list objects, as registered defaults are.
        unsafe { NSUserDefaults::standardUserDefaults().registerDefaults(&defaults) };
        NSWindow::setAllowsAutomaticWindowTabbing(false, mtm);
        let bar = Self { target, mtm };
        bar.show(&SystemBar::default());
        Some(bar)
    }

    fn item(&self, title: &str, action: Option<Sel>, key: &str) -> Retained<NSMenuItem> {
        // SAFETY: the action is one of the target's, of NSApplication's
        // standard ones, or none.
        unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                self.mtm.alloc(),
                &NSString::from_str(title),
                action,
                &NSString::from_str(key),
            )
        }
    }

    /// An item of the application's, chosen through the target.
    fn app_item(
        &self,
        item: &SystemItem,
        title: &str,
        fallback_key: Option<KeyEquivalent>,
    ) -> Retained<NSMenuItem> {
        let made = self.item(title, Some(sel!(choose:)), "");
        // SAFETY: the target lives as long as the service, which outlives
        // the menus it makes.
        unsafe { made.setTarget(Some(&self.target)) };
        made.setTag(item.id as NSInteger);
        made.setEnabled(item.enabled);
        if item.checkable {
            made.setState(if item.checked {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        if let Some(key) = item.key.or(fallback_key) {
            set_key(&made, key);
        }
        made
    }

    fn menu(&self, menu: &SystemMenu) -> Retained<NSMenu> {
        let made = NSMenu::initWithTitle(self.mtm.alloc(), &NSString::from_str(&menu.title));
        // What an item says of itself is the application's.
        made.setAutoenablesItems(false);
        for entry in &menu.entries {
            match entry {
                SystemEntry::Item(item) => made.addItem(&self.app_item(item, &item.text, None)),
                SystemEntry::Separator => made.addItem(&NSMenuItem::separatorItem(self.mtm)),
                SystemEntry::Menu(sub) => {
                    let holder = self.item(&sub.title, None, "");
                    holder.setSubmenu(Some(&self.menu(sub)));
                    made.addItem(&holder);
                }
            }
        }
        made
    }

    fn application_menu(&self, bar: &SystemBar, app: &NSMenu, name: &str) {
        let separator = || NSMenuItem::separatorItem(self.mtm);
        let command = |c: char| KeyEquivalent {
            key: c as i32,
            command: true,
            shift: false,
            option: false,
            control: false,
        };
        if let Some(about) = &bar.about {
            app.addItem(&self.app_item(about, &format!("About {name}"), None));
            app.addItem(&separator());
        }
        if let Some(preferences) = &bar.preferences {
            app.addItem(&self.app_item(preferences, "Preferences…", Some(command(','))));
            app.addItem(&separator());
        }
        let services = self.item("Services", None, "");
        let services_menu =
            NSMenu::initWithTitle(self.mtm.alloc(), &NSString::from_str("Services"));
        services.setSubmenu(Some(&services_menu));
        NSApplication::sharedApplication(self.mtm).setServicesMenu(Some(&services_menu));
        app.addItem(&services);
        app.addItem(&separator());
        app.addItem(&self.item(&format!("Hide {name}"), Some(sel!(hide:)), "h"));
        let others = self.item("Hide Others", Some(sel!(hideOtherApplications:)), "h");
        others.setKeyEquivalentModifierMask(
            NSEventModifierFlags::Command | NSEventModifierFlags::Option,
        );
        app.addItem(&others);
        app.addItem(&self.item("Show All", Some(sel!(unhideAllApplications:)), ""));
        app.addItem(&separator());
        let quit = self.item(&format!("Quit {name}"), Some(sel!(quit:)), "q");
        // SAFETY: as in `app_item`.
        unsafe { quit.setTarget(Some(&self.target)) };
        app.addItem(&quit);
    }
}

impl MenuBarService for AppKitMenuBar {
    fn native(&self) -> bool {
        true
    }

    fn show(&self, bar: &SystemBar) {
        let name = NSProcessInfo::processInfo().processName().to_string();
        let main = NSMenu::initWithTitle(self.mtm.alloc(), &NSString::from_str(""));
        let app = NSMenu::initWithTitle(self.mtm.alloc(), &NSString::from_str(&name));
        self.application_menu(bar, &app, &name);
        let holder = self.item(&name, None, "");
        holder.setSubmenu(Some(&app));
        main.addItem(&holder);
        let app_kit = NSApplication::sharedApplication(self.mtm);
        let mut windows = None;
        for menu in &bar.menus {
            let holder = self.item(&menu.title, None, "");
            let made = self.menu(menu);
            holder.setSubmenu(Some(&made));
            main.addItem(&holder);
            // The menu macOS lists the application's windows in, as every
            // Cocoa application's Window menu does.
            if menu.title == "Window" {
                windows = Some(made);
            }
        }
        app_kit.setMainMenu(Some(&main));
        app_kit.setWindowsMenu(windows.as_deref());
    }
}

fn set_key(item: &NSMenuItem, key: KeyEquivalent) {
    let Some(c) = key_char(key.key) else {
        return;
    };
    item.setKeyEquivalent(&NSString::from_str(&c.to_string()));
    let mut mask = NSEventModifierFlags::empty();
    for (on, flag) in [
        (key.command, NSEventModifierFlags::Command),
        (key.shift, NSEventModifierFlags::Shift),
        (key.option, NSEventModifierFlags::Option),
        (key.control, NSEventModifierFlags::Control),
    ] {
        if on {
            mask |= flag;
        }
    }
    item.setKeyEquivalentModifierMask(mask);
}

/// The character AppKit names a key by in a key equivalent: a letter in
/// lower case (Shift is in the mask), the private-use characters of
/// NSEvent's function keys for the keys that have no character.
fn key_char(key: i32) -> Option<char> {
    let function = |n: u32| char::from_u32(0xF700 + n);
    match key {
        0x41..=0x5a => char::from_u32(key as u32 + 0x20),
        0x20..=0x7e => char::from_u32(key as u32),
        0x0100_0000 => Some('\u{1b}'), // Escape
        0x0100_0001 => Some('\t'),     // Tab
        0x0100_0003 => Some('\u{8}'),  // Backspace
        0x0100_0004 => Some('\r'),     // Return
        0x0100_0005 => Some('\u{3}'),  // Enter
        0x0100_0007 => function(0x28), // Delete
        0x0100_0010 => function(0x29), // Home
        0x0100_0011 => function(0x2b), // End
        0x0100_0012 => function(0x02), // Left
        0x0100_0013 => function(0x00), // Up
        0x0100_0014 => function(0x03), // Right
        0x0100_0015 => function(0x01), // Down
        0x0100_0016 => function(0x2c), // Page Up
        0x0100_0017 => function(0x2d), // Page Down
        0x0100_0030..=0x0100_0052 => function(0x04 + (key - 0x0100_0030) as u32), // F1–F35
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::key_char;

    #[test]
    fn keys_are_named_as_appkit_names_them() {
        assert_eq!(key_char('N' as i32), Some('n'));
        assert_eq!(key_char(',' as i32), Some(','));
        assert_eq!(key_char(0x0100_0030), Some('\u{F704}'));
        assert_eq!(key_char(0x0100_0013), Some('\u{F700}'));
        assert_eq!(key_char(0x0100_00ff), None);
    }
}
