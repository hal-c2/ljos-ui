//! Key codes: the `Qt.Key` numbers a [`crate::KeyEvent`] carries, with
//! names for the ones Rust code uses, and the names a key sequence is
//! written with (doc.qt.io/qt-6/qt.html#Key-enum,
//! doc.qt.io/qt-6/qkeysequence.html).
//!
//! A key that prints a Latin-1 character has that character's code, a
//! letter its upper case; QML reads every one as `Qt.Key_*`.

pub const SPACE: i32 = 0x20;
pub const ASTERISK: i32 = 0x2a;
pub const PLUS: i32 = 0x2b;
pub const COMMA: i32 = 0x2c;
pub const MINUS: i32 = 0x2d;
pub const DIGIT_0: i32 = 0x30;
pub const DIGIT_9: i32 = 0x39;
pub const A: i32 = 0x41;
pub const Z: i32 = 0x5a;

pub const ESCAPE: i32 = 0x0100_0000;
pub const TAB: i32 = 0x0100_0001;
pub const BACKTAB: i32 = 0x0100_0002;
pub const BACKSPACE: i32 = 0x0100_0003;
pub const RETURN: i32 = 0x0100_0004;
/// The Enter key of the keypad.
pub const ENTER: i32 = 0x0100_0005;
pub const INSERT: i32 = 0x0100_0006;
pub const DELETE: i32 = 0x0100_0007;
pub const PAUSE: i32 = 0x0100_0008;
pub const PRINT: i32 = 0x0100_0009;
pub const SYS_REQ: i32 = 0x0100_000a;
pub const CLEAR: i32 = 0x0100_000b;
pub const HOME: i32 = 0x0100_0010;
pub const END: i32 = 0x0100_0011;
pub const LEFT: i32 = 0x0100_0012;
pub const UP: i32 = 0x0100_0013;
pub const RIGHT: i32 = 0x0100_0014;
pub const DOWN: i32 = 0x0100_0015;
pub const PAGE_UP: i32 = 0x0100_0016;
pub const PAGE_DOWN: i32 = 0x0100_0017;
pub const SHIFT: i32 = 0x0100_0020;
/// The Ctrl key; on macOS the Command key, as in Qt.
pub const CONTROL: i32 = 0x0100_0021;
pub const META: i32 = 0x0100_0022;
pub const ALT: i32 = 0x0100_0023;
pub const CAPS_LOCK: i32 = 0x0100_0024;
pub const NUM_LOCK: i32 = 0x0100_0025;
pub const SCROLL_LOCK: i32 = 0x0100_0026;
pub const F1: i32 = 0x0100_0030;
/// The edit key on most desktops.
pub const F2: i32 = 0x0100_0031;
pub const F35: i32 = 0x0100_0052;
pub const SUPER_L: i32 = 0x0100_0053;
pub const SUPER_R: i32 = 0x0100_0054;
pub const MENU: i32 = 0x0100_0055;
pub const HYPER_L: i32 = 0x0100_0056;
pub const HYPER_R: i32 = 0x0100_0057;
pub const HELP: i32 = 0x0100_0058;
pub const BACK: i32 = 0x0100_0061;
pub const FORWARD: i32 = 0x0100_0062;
pub const STOP: i32 = 0x0100_0063;
pub const REFRESH: i32 = 0x0100_0064;
pub const VOLUME_DOWN: i32 = 0x0100_0070;
pub const VOLUME_MUTE: i32 = 0x0100_0071;
pub const VOLUME_UP: i32 = 0x0100_0072;
pub const MEDIA_PLAY: i32 = 0x0100_0080;
pub const MEDIA_STOP: i32 = 0x0100_0081;
pub const MEDIA_PREVIOUS: i32 = 0x0100_0082;
pub const MEDIA_NEXT: i32 = 0x0100_0083;
pub const MEDIA_RECORD: i32 = 0x0100_0084;
pub const MEDIA_PAUSE: i32 = 0x0100_0085;
pub const HOME_PAGE: i32 = 0x0100_0090;
pub const FAVORITES: i32 = 0x0100_0091;
pub const SEARCH: i32 = 0x0100_0092;
pub const STANDBY: i32 = 0x0100_0093;
pub const OPEN_URL: i32 = 0x0100_0094;
pub const LAUNCH_MAIL: i32 = 0x0100_00a0;
pub const LAUNCH_MEDIA: i32 = 0x0100_00a1;
pub const ZOOM_IN: i32 = 0x0100_00f6;
pub const ZOOM_OUT: i32 = 0x0100_00f7;
pub const ALT_GR: i32 = 0x0100_1103;
pub const PRINTER: i32 = 0x0102_0002;
pub const SELECT: i32 = 0x0101_0000;
pub const YES: i32 = 0x0101_0001;
pub const NO: i32 = 0x0101_0002;
pub const CANCEL: i32 = 0x0102_0001;
pub const CONTEXT_1: i32 = 0x0110_0000;
pub const CONTEXT_2: i32 = 0x0110_0001;
pub const CONTEXT_3: i32 = 0x0110_0002;
pub const CONTEXT_4: i32 = 0x0110_0003;
pub const CALL: i32 = 0x0110_0004;
pub const HANGUP: i32 = 0x0110_0005;
pub const FLIP: i32 = 0x0110_0006;
pub const COPY: i32 = 0x0100_00cf;
pub const CUT: i32 = 0x0100_00d0;
pub const PASTE: i32 = 0x0100_00e2;
pub const UNKNOWN: i32 = 0x01ff_ffff;

/// Whether the key is a modifier by itself (Shift, Ctrl, …): such a key
/// is an event of its own but never a shortcut.
pub fn is_modifier(key: i32) -> bool {
    matches!(
        key,
        SHIFT
            | CONTROL
            | META
            | ALT
            | ALT_GR
            | CAPS_LOCK
            | NUM_LOCK
            | SCROLL_LOCK
            | SUPER_L
            | SUPER_R
            | HYPER_L
            | HYPER_R
    )
}

/// The `Qt.Key` of a character: itself for Latin-1, upper case for a
/// letter. `None` for a character no key has.
pub fn from_char(c: char) -> Option<i32> {
    // A letter whose upper case is several characters (ß) stays itself.
    let mut upper = c.to_uppercase();
    let c = match (upper.next(), upper.next()) {
        (Some(u), None) => u,
        _ => c,
    };
    let n = u32::from(c);
    (0x20..=0xff)
        .contains(&n)
        .then_some(n as i32)
        .filter(|n| !(0x7f..0xa0).contains(n))
}

/// The keys with a name in a key sequence, with the name Qt writes first
/// and the others it reads.
const NAMED: &[(i32, &[&str])] = &[
    (SPACE, &["Space"]),
    (ESCAPE, &["Esc", "Escape"]),
    (TAB, &["Tab"]),
    (BACKTAB, &["Backtab"]),
    (BACKSPACE, &["Backspace"]),
    (RETURN, &["Return"]),
    (ENTER, &["Enter"]),
    (INSERT, &["Ins", "Insert"]),
    (DELETE, &["Del", "Delete"]),
    (PAUSE, &["Pause"]),
    (PRINT, &["Print"]),
    (SYS_REQ, &["SysReq"]),
    (CLEAR, &["Clear"]),
    (HOME, &["Home"]),
    (END, &["End"]),
    (LEFT, &["Left"]),
    (UP, &["Up"]),
    (RIGHT, &["Right"]),
    (DOWN, &["Down"]),
    (PAGE_UP, &["PgUp", "Page Up"]),
    (PAGE_DOWN, &["PgDown", "Page Down"]),
    (SHIFT, &["Shift"]),
    (META, &["Meta"]),
    (ALT, &["Alt"]),
    (CAPS_LOCK, &["CapsLock"]),
    (NUM_LOCK, &["NumLock"]),
    (SCROLL_LOCK, &["ScrollLock"]),
    (MENU, &["Menu"]),
    (HELP, &["Help"]),
    (BACK, &["Back"]),
    (FORWARD, &["Forward"]),
    (STOP, &["Stop"]),
    (REFRESH, &["Refresh"]),
    (VOLUME_DOWN, &["Volume Down"]),
    (VOLUME_MUTE, &["Volume Mute"]),
    (VOLUME_UP, &["Volume Up"]),
    (SELECT, &["Select"]),
    (YES, &["Yes"]),
    (NO, &["No"]),
    (CANCEL, &["Cancel"]),
    (CONTEXT_1, &["Context1"]),
    (CONTEXT_2, &["Context2"]),
    (CONTEXT_3, &["Context3"]),
    (CONTEXT_4, &["Context4"]),
    (CALL, &["Call"]),
    (HANGUP, &["Hangup"]),
    (FLIP, &["Flip"]),
    (MEDIA_PLAY, &["Media Play"]),
    (MEDIA_STOP, &["Media Stop"]),
    (MEDIA_PREVIOUS, &["Media Previous"]),
    (MEDIA_NEXT, &["Media Next"]),
    (MEDIA_RECORD, &["Media Record"]),
    (MEDIA_PAUSE, &["Media Pause"]),
    (HOME_PAGE, &["Home Page"]),
    (FAVORITES, &["Favorites"]),
    (SEARCH, &["Search"]),
    (STANDBY, &["Standby"]),
    (OPEN_URL, &["Open URL"]),
    (LAUNCH_MAIL, &["Launch Mail"]),
    (LAUNCH_MEDIA, &["Launch Media"]),
    (ZOOM_IN, &["Zoom In"]),
    (ZOOM_OUT, &["Zoom Out"]),
    (PRINTER, &["Printer"]),
];

/// The key a name in a key sequence stands for: `"Del"`, `"F5"`, `"S"`.
/// Names are read without regard to case, as Qt does.
pub fn from_name(name: &str) -> Option<i32> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return from_char(c);
    }
    if let Some((k, _)) = NAMED
        .iter()
        .find(|(_, names)| names.iter().any(|n| n.eq_ignore_ascii_case(name)))
    {
        return Some(*k);
    }
    let n: i32 = name
        .strip_prefix(['F', 'f'])
        .filter(|d| !d.starts_with('0') && d.bytes().all(|b| b.is_ascii_digit()))?
        .parse()
        .ok()?;
    (1..=35).contains(&n).then(|| F1 + n - 1)
}

/// The name Qt writes for a key in a portable key sequence. `None` for a
/// key that has none.
pub fn name(key: i32) -> Option<String> {
    if let Some((_, names)) = NAMED.iter().find(|(k, _)| *k == key) {
        return names.first().map(|n| (*n).to_string());
    }
    if (F1..=F35).contains(&key) {
        return Some(format!("F{}", key - F1 + 1));
    }
    u32::try_from(key)
        .ok()
        .filter(|n| (0x21..=0xff).contains(n))
        .and_then(char::from_u32)
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_letter_is_its_upper_case_code() {
        assert_eq!(from_char('a'), Some(A));
        assert_eq!(from_char('Z'), Some(Z));
        assert_eq!(from_char('7'), Some(0x37));
        assert_eq!(from_char('*'), Some(ASTERISK));
        assert_eq!(from_char('\u{1}'), None);
        assert_eq!(from_char('λ'), None);
    }

    #[test]
    fn names_are_read_in_any_case_and_written_as_qt_does() {
        assert_eq!(from_name("del"), Some(DELETE));
        assert_eq!(from_name("Delete"), Some(DELETE));
        assert_eq!(from_name("PgDown"), Some(PAGE_DOWN));
        assert_eq!(from_name("Page Up"), Some(PAGE_UP));
        assert_eq!(from_name("PageUp"), None);
        assert_eq!(from_name("Plus"), None);
        assert_eq!(from_name("+"), Some(PLUS));
        assert_eq!(from_name("F12"), Some(F1 + 11));
        assert_eq!(from_name("F35"), Some(F35));
        assert_eq!(from_name("F36"), None);
        assert_eq!(from_name("F0"), None);
        assert_eq!(from_name("s"), Some(0x53));
        assert_eq!(from_name("Nonsense"), None);
        assert_eq!(name(DELETE).as_deref(), Some("Del"));
        assert_eq!(name(ESCAPE).as_deref(), Some("Esc"));
        assert_eq!(name(F1 + 2).as_deref(), Some("F3"));
        assert_eq!(name(0x53).as_deref(), Some("S"));
        assert_eq!(name(PLUS).as_deref(), Some("+"));
        assert_eq!(name(SHIFT).as_deref(), Some("Shift"));
        assert_eq!(name(UNKNOWN), None);
    }
}
