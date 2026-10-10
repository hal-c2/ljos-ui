//! Qt's accessible roles and what each becomes in AccessKit.
//!
//! The names and numbers are the documented `QAccessible::Role` enum
//! (doc.qt.io/qt-6/qaccessible.html), which is what `Accessible.role`
//! takes in QML. The mapping is the table in
//! `docs/research/05-accessibility-and-localisation.md` §2.4.

use accesskit::Role as Ak;

macro_rules! roles {
    ($($name:ident = $value:literal => $ak:expr,)*) => {
        /// A `QAccessible::Role`, the value of `Accessible.role`.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub enum Role {
            #[default]
            $($name,)*
        }

        /// Every role with its QML key, Qt's number and its AccessKit
        /// role. `None` means the item gets no node of its own.
        pub const ROLES: &[RoleInfo] = &[
            $(RoleInfo {
                role: Role::$name,
                key: stringify!($name),
                qt: $value,
                accesskit: $ak,
            },)*
        ];
    };
}

/// One row of [`ROLES`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoleInfo {
    pub role: Role,
    /// The key QML writes: `Accessible.Button`.
    pub key: &'static str,
    /// Qt's value for the key.
    pub qt: i32,
    pub accesskit: Option<Ak>,
}

roles! {
    NoRole = 0x00 => None,
    TitleBar = 0x01 => Some(Ak::TitleBar),
    MenuBar = 0x02 => Some(Ak::MenuBar),
    ScrollBar = 0x03 => Some(Ak::ScrollBar),
    Grip = 0x04 => Some(Ak::Splitter),
    Sound = 0x05 => Some(Ak::Audio),
    Cursor = 0x06 => None,
    Caret = 0x07 => None,
    AlertMessage = 0x08 => Some(Ak::Alert),
    Window = 0x09 => Some(Ak::Window),
    Client = 0x0A => Some(Ak::GenericContainer),
    PopupMenu = 0x0B => Some(Ak::Menu),
    MenuItem = 0x0C => Some(Ak::MenuItem),
    ToolTip = 0x0D => Some(Ak::Tooltip),
    Application = 0x0E => Some(Ak::Application),
    Document = 0x0F => Some(Ak::Document),
    Pane = 0x10 => Some(Ak::Pane),
    Chart = 0x11 => Some(Ak::Image),
    Dialog = 0x12 => Some(Ak::Dialog),
    Border = 0x13 => Some(Ak::GenericContainer),
    Grouping = 0x14 => Some(Ak::Group),
    Separator = 0x15 => Some(Ak::Splitter),
    ToolBar = 0x16 => Some(Ak::Toolbar),
    StatusBar = 0x17 => Some(Ak::Status),
    Table = 0x18 => Some(Ak::Table),
    ColumnHeader = 0x19 => Some(Ak::ColumnHeader),
    RowHeader = 0x1A => Some(Ak::RowHeader),
    // AccessKit has no column role: a table's columns are its cells' indices.
    Column = 0x1B => Some(Ak::GenericContainer),
    Row = 0x1C => Some(Ak::Row),
    Cell = 0x1D => Some(Ak::Cell),
    Link = 0x1E => Some(Ak::Link),
    HelpBalloon = 0x1F => Some(Ak::Tooltip),
    Assistant = 0x20 => Some(Ak::GenericContainer),
    List = 0x21 => Some(Ak::List),
    ListItem = 0x22 => Some(Ak::ListItem),
    Tree = 0x23 => Some(Ak::Tree),
    TreeItem = 0x24 => Some(Ak::TreeItem),
    PageTab = 0x25 => Some(Ak::Tab),
    PropertyPage = 0x26 => Some(Ak::TabPanel),
    Indicator = 0x27 => Some(Ak::Meter),
    Graphic = 0x28 => Some(Ak::Image),
    StaticText = 0x29 => Some(Ak::Label),
    EditableText = 0x2A => Some(Ak::TextInput),
    Button = 0x2B => Some(Ak::Button),
    CheckBox = 0x2C => Some(Ak::CheckBox),
    RadioButton = 0x2D => Some(Ak::RadioButton),
    ComboBox = 0x2E => Some(Ak::ComboBox),
    ProgressBar = 0x30 => Some(Ak::ProgressIndicator),
    Dial = 0x31 => Some(Ak::Slider),
    HotkeyField = 0x32 => Some(Ak::TextInput),
    Slider = 0x33 => Some(Ak::Slider),
    SpinBox = 0x34 => Some(Ak::SpinButton),
    Canvas = 0x35 => Some(Ak::Canvas),
    Animation = 0x36 => Some(Ak::Image),
    Equation = 0x37 => Some(Ak::Math),
    ButtonDropDown = 0x38 => Some(Ak::Button),
    ButtonMenu = 0x39 => Some(Ak::Button),
    ButtonDropGrid = 0x3A => Some(Ak::Button),
    Whitespace = 0x3B => None,
    PageTabList = 0x3C => Some(Ak::TabList),
    Clock = 0x3D => Some(Ak::Time),
    Splitter = 0x3E => Some(Ak::Splitter),
    LayeredPane = 0x80 => Some(Ak::GenericContainer),
    Terminal = 0x81 => Some(Ak::Terminal),
    Desktop = 0x82 => Some(Ak::Pane),
    Paragraph = 0x83 => Some(Ak::Paragraph),
    WebDocument = 0x84 => Some(Ak::Document),
    Section = 0x85 => Some(Ak::Section),
    Notification = 0x86 => Some(Ak::Status),
    Switch = 0x87 => Some(Ak::Switch),
    ColorChooser = 0x404 => Some(Ak::ColorWell),
    Footer = 0x40E => Some(Ak::Footer),
    Form = 0x410 => Some(Ak::Form),
    Heading = 0x414 => Some(Ak::Heading),
    Note = 0x41B => Some(Ak::Note),
    ComplementaryContent = 0x42C => Some(Ak::Complementary),
    BlockQuote = 0x431 => Some(Ak::Blockquote),
    UserRole = 0xFFFF => Some(Ak::Unknown),
}

/// Keys Qt gives a second name: `Accessible.PushButton` is
/// `Accessible.Button`.
pub const ROLE_ALIASES: &[(&str, Role)] = &[("PushButton", Role::Button)];

impl Role {
    fn info(self) -> &'static RoleInfo {
        // The macro lists the variants in table order.
        &ROLES[self as usize]
    }

    /// The role with Qt's number `value`. Numbers Qt does not list are
    /// `None`; an application's own roles start at `UserRole` and all read
    /// as it.
    pub fn from_qt(value: i32) -> Option<Role> {
        if value > Role::UserRole.qt_value() {
            return Some(Role::UserRole);
        }
        ROLES.iter().find(|r| r.qt == value).map(|r| r.role)
    }

    /// Qt's number for the role.
    pub fn qt_value(self) -> i32 {
        self.info().qt
    }

    /// The key QML writes after `Accessible.`.
    pub fn key(self) -> &'static str {
        self.info().key
    }

    /// The AccessKit role, or `None` for the roles that say "nothing is
    /// here" (`NoRole`, `Whitespace`, `Cursor`, `Caret`).
    pub fn accesskit(self) -> Option<Ak> {
        self.info().accesskit
    }

    /// Qt's documented default of `Accessible.focusable`: true for these
    /// roles, false for every other.
    pub fn focusable_by_default(self) -> bool {
        matches!(
            self,
            Role::CheckBox
                | Role::RadioButton
                | Role::Switch
                | Role::Button
                | Role::MenuItem
                | Role::PageTab
                | Role::EditableText
                | Role::SpinBox
                | Role::ComboBox
                | Role::Terminal
                | Role::ScrollBar
        )
    }

    /// Whether an assistive technology can press the item. Qt documents
    /// the press action for buttons, check boxes, radio buttons and
    /// switches; the rest are the other roles a pointer click activates.
    pub fn pressable(self) -> bool {
        matches!(
            self,
            Role::Button
                | Role::ButtonMenu
                | Role::ButtonDropDown
                | Role::ButtonDropGrid
                | Role::CheckBox
                | Role::RadioButton
                | Role::Switch
                | Role::MenuItem
                | Role::PageTab
                | Role::Link
        )
    }
}

/// Every `Accessible.<key>` role key with Qt's number, aliases included:
/// the enum table a QML engine registers.
pub fn qt_role_keys() -> Vec<(&'static str, i32)> {
    ROLES
        .iter()
        .map(|r| (r.key, r.qt))
        .chain(ROLE_ALIASES.iter().map(|(k, r)| (*k, r.qt_value())))
        .collect()
}

/// The lower-case word for an AccessKit role, as tests and tools name
/// roles: `button`, `checkbox`, `label`, `progressindicator`.
pub fn role_word(role: Ak) -> String {
    format!("{role:?}").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_in_enum_order() {
        // `Role::info` indexes by discriminant.
        for (i, r) in ROLES.iter().enumerate() {
            assert_eq!(r.role as usize, i, "{}", r.key);
            assert_eq!(r.role.key(), r.key);
            assert_eq!(format!("{:?}", r.role), r.key);
        }
    }

    #[test]
    fn qt_numbers_are_unique_and_round_trip() {
        let mut seen = std::collections::HashSet::new();
        for r in ROLES {
            assert!(seen.insert(r.qt), "{} repeats 0x{:x}", r.key, r.qt);
            assert_eq!(Role::from_qt(r.qt), Some(r.role));
            assert_eq!(r.role.qt_value(), r.qt);
        }
    }

    #[test]
    fn documented_numbers() {
        // Spot checks against doc.qt.io/qt-6/qaccessible.html#Role-enum.
        for (role, qt) in [
            (Role::NoRole, 0x00),
            (Role::AlertMessage, 0x08),
            (Role::StaticText, 0x29),
            (Role::EditableText, 0x2A),
            (Role::Button, 0x2B),
            (Role::CheckBox, 0x2C),
            (Role::ComboBox, 0x2E),
            (Role::ProgressBar, 0x30),
            (Role::Splitter, 0x3E),
            (Role::LayeredPane, 0x80),
            (Role::Switch, 0x87),
            (Role::ColorChooser, 0x404),
            (Role::BlockQuote, 0x431),
            (Role::UserRole, 0xFFFF),
        ] {
            assert_eq!(role.qt_value(), qt, "{role:?}");
        }
    }

    #[test]
    fn numbers_qt_does_not_list() {
        // 0x2F is a hole in Qt's enum.
        assert_eq!(Role::from_qt(0x2F), None);
        assert_eq!(Role::from_qt(-1), None);
        assert_eq!(Role::from_qt(0x1_0000), Some(Role::UserRole));
    }

    #[test]
    fn the_mapping_table() {
        // Report 05 §2.4, row by row.
        let want = [
            (Role::Button, Ak::Button),
            (Role::CheckBox, Ak::CheckBox),
            (Role::RadioButton, Ak::RadioButton),
            (Role::Switch, Ak::Switch),
            (Role::Slider, Ak::Slider),
            (Role::Dial, Ak::Slider),
            (Role::SpinBox, Ak::SpinButton),
            (Role::ScrollBar, Ak::ScrollBar),
            (Role::ComboBox, Ak::ComboBox),
            (Role::EditableText, Ak::TextInput),
            (Role::StaticText, Ak::Label),
            (Role::Heading, Ak::Heading),
            (Role::Paragraph, Ak::Paragraph),
            (Role::BlockQuote, Ak::Blockquote),
            (Role::Note, Ak::Note),
            (Role::Link, Ak::Link),
            (Role::Graphic, Ak::Image),
            (Role::Animation, Ak::Image),
            (Role::Chart, Ak::Image),
            (Role::Canvas, Ak::Canvas),
            (Role::List, Ak::List),
            (Role::ListItem, Ak::ListItem),
            (Role::Tree, Ak::Tree),
            (Role::TreeItem, Ak::TreeItem),
            (Role::Table, Ak::Table),
            (Role::Row, Ak::Row),
            (Role::Cell, Ak::Cell),
            (Role::ColumnHeader, Ak::ColumnHeader),
            (Role::RowHeader, Ak::RowHeader),
            (Role::PageTab, Ak::Tab),
            (Role::PageTabList, Ak::TabList),
            (Role::PropertyPage, Ak::TabPanel),
            (Role::MenuBar, Ak::MenuBar),
            (Role::PopupMenu, Ak::Menu),
            (Role::MenuItem, Ak::MenuItem),
            (Role::ButtonMenu, Ak::Button),
            (Role::ButtonDropDown, Ak::Button),
            (Role::ToolBar, Ak::Toolbar),
            (Role::ToolTip, Ak::Tooltip),
            (Role::HelpBalloon, Ak::Tooltip),
            (Role::ProgressBar, Ak::ProgressIndicator),
            (Role::Indicator, Ak::Meter),
            (Role::Dialog, Ak::Dialog),
            (Role::AlertMessage, Ak::Alert),
            (Role::Notification, Ak::Status),
            (Role::StatusBar, Ak::Status),
            (Role::Terminal, Ak::Terminal),
            (Role::Window, Ak::Window),
            (Role::TitleBar, Ak::TitleBar),
            (Role::Application, Ak::Application),
            (Role::Pane, Ak::Pane),
            (Role::Client, Ak::GenericContainer),
            (Role::Assistant, Ak::GenericContainer),
            (Role::LayeredPane, Ak::GenericContainer),
            (Role::Grouping, Ak::Group),
            (Role::Section, Ak::Section),
            (Role::Form, Ak::Form),
            (Role::Footer, Ak::Footer),
            (Role::ComplementaryContent, Ak::Complementary),
            (Role::Document, Ak::Document),
            (Role::WebDocument, Ak::Document),
            (Role::Separator, Ak::Splitter),
            (Role::Splitter, Ak::Splitter),
            (Role::Grip, Ak::Splitter),
            (Role::Equation, Ak::Math),
            (Role::ColorChooser, Ak::ColorWell),
            (Role::HotkeyField, Ak::TextInput),
            (Role::Sound, Ak::Audio),
            (Role::UserRole, Ak::Unknown),
        ];
        for (qt, ak) in want {
            assert_eq!(qt.accesskit(), Some(ak), "{qt:?}");
        }
        for none in [Role::NoRole, Role::Whitespace, Role::Cursor, Role::Caret] {
            assert_eq!(none.accesskit(), None, "{none:?}");
        }
    }

    #[test]
    fn focusable_defaults_follow_the_documentation() {
        let on = [
            Role::CheckBox,
            Role::RadioButton,
            Role::Switch,
            Role::Button,
            Role::MenuItem,
            Role::PageTab,
            Role::EditableText,
            Role::SpinBox,
            Role::ComboBox,
            Role::Terminal,
            Role::ScrollBar,
        ];
        for r in ROLES {
            assert_eq!(
                r.role.focusable_by_default(),
                on.contains(&r.role),
                "{}",
                r.key
            );
        }
    }

    #[test]
    fn keys_include_the_alias() {
        let keys = qt_role_keys();
        assert_eq!(keys.len(), ROLES.len() + 1);
        let get = |k: &str| keys.iter().find(|(n, _)| *n == k).map(|(_, v)| *v);
        assert_eq!(get("Button"), Some(0x2B));
        assert_eq!(get("PushButton"), Some(0x2B));
        assert_eq!(get("NoRole"), Some(0));
    }

    #[test]
    fn role_words_are_lower_case() {
        assert_eq!(role_word(Ak::Button), "button");
        assert_eq!(role_word(Ak::CheckBox), "checkbox");
        assert_eq!(role_word(Ak::Label), "label");
        assert_eq!(role_word(Ak::ProgressIndicator), "progressindicator");
    }
}
