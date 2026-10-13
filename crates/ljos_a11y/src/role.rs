//! Qt's accessible roles and what each becomes in AccessKit.
//!
//! The names and numbers are the documented `QAccessible::Role` enum
//! (doc.qt.io/qt-6/qaccessible.html), which is what `Accessible.role`
//! takes in QML. The mapping is the table in
//! `docs/research/05-accessibility-and-localisation.md` §2.4.

use accesskit::Role as Ak;

use crate::tree::Politeness;

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

    /// Qt's default of `Accessible.focusable`: true for these roles, false
    /// for every other. The documentation's list has no `StaticText`, but
    /// Qt 6.11 answers true for it (a screen reader walks the text of a
    /// window with its own focus), and what Qt answers is what authors'
    /// code sees.
    pub fn focusable_by_default(self) -> bool {
        matches!(
            self,
            Role::StaticText
                | Role::CheckBox
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
                | Role::ComboBox
                | Role::ListItem
        )
    }
}

/// What an item says of itself that changes which AccessKit role its Qt
/// role becomes: Qt has one `EditableText` where AccessKit tells a search
/// field from a password field. The flags are the `Accessible` properties
/// of the same names; `exclusive` has none and is for a control that knows
/// it is one of a group of which one is checked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoleFlags {
    pub default_button: bool,
    pub editable: bool,
    pub multi_line: bool,
    pub password_edit: bool,
    pub search_edit: bool,
    pub selectable: bool,
    pub checkable: bool,
    pub exclusive: bool,
}

impl Role {
    /// The AccessKit role of an item with these flags
    /// (`docs/research/05-accessibility-and-localisation.md` §2.4, the
    /// "refinements" column). Without flags this is [`Role::accesskit`].
    pub fn refined(self, flags: RoleFlags) -> Option<Ak> {
        let plain = self.accesskit()?;
        Some(match self {
            Role::Button if flags.default_button => Ak::DefaultButton,
            Role::ComboBox if flags.editable => Ak::EditableComboBox,
            // A field that hides what is typed is that before anything
            // else: a screen reader must not echo it.
            Role::EditableText if flags.password_edit => Ak::PasswordInput,
            Role::EditableText if flags.search_edit => Ak::SearchInput,
            Role::EditableText if flags.multi_line => Ak::MultilineTextInput,
            Role::List if flags.selectable => Ak::ListBox,
            Role::ListItem if flags.selectable => Ak::ListBoxOption,
            Role::MenuItem if flags.checkable && flags.exclusive => Ak::MenuItemRadio,
            Role::MenuItem if flags.checkable => Ak::MenuItemCheckBox,
            _ => plain,
        })
    }

    /// What a screen reader should call the role when AccessKit's word for
    /// it says less than Qt's: a dial is a slider there.
    pub fn role_description(self) -> Option<&'static str> {
        match self {
            Role::Dial => Some("dial"),
            _ => None,
        }
    }

    /// Qt's default of `Accessible.checkable`: what Qt 6.11 answers for an
    /// item that has the role and was told nothing else.
    pub fn checkable_by_default(self) -> bool {
        matches!(self, Role::CheckBox | Role::RadioButton | Role::Switch)
    }

    /// Qt's default of `Accessible.editable`, found the same way.
    pub fn editable_by_default(self) -> bool {
        self == Role::EditableText
    }

    /// Qt's default of `Accessible.readOnly`, found the same way.
    pub fn read_only_by_default(self) -> bool {
        self == Role::StaticText
    }

    /// Whether the role is of something a user acts on: it needs a name,
    /// and room for a finger (the audit asks both).
    pub fn actionable(self) -> bool {
        self.pressable()
            || matches!(
                self,
                Role::EditableText
                    | Role::SpinBox
                    | Role::Slider
                    | Role::Dial
                    | Role::ScrollBar
                    | Role::HotkeyField
                    | Role::TreeItem
            )
    }

    /// Whether the role is of one of a set of the same: a row, an item of
    /// a menu, a tab. Such an item has a place in its set to tell.
    pub fn in_set(self) -> bool {
        matches!(
            self,
            Role::ListItem
                | Role::TreeItem
                | Role::MenuItem
                | Role::PageTab
                | Role::RadioButton
                | Role::Row
        )
    }

    /// Whether the role is a picture: it has a name, or it is left out.
    pub fn image(self) -> bool {
        matches!(
            self,
            Role::Graphic | Role::Chart | Role::Animation | Role::Canvas
        )
    }

    /// Whether what a node of this role says is said again when it
    /// changes, without the user going to it, and how urgently: a status
    /// bar or a notification politely, an alert at once. These are the
    /// roles whose AccessKit counterparts are live regions on every
    /// platform (report 05 §2.4); Qt's own bridge says nothing of them.
    pub fn live(self) -> Option<Politeness> {
        match self {
            Role::StatusBar | Role::Notification => Some(Politeness::Polite),
            Role::AlertMessage => Some(Politeness::Assertive),
            _ => None,
        }
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

    /// The whole of doc.qt.io/qt-6/qaccessible.html#Role-enum, in the
    /// page's order: every constant and its number. A role Qt documents
    /// and the table lacks fails here, and so does one the table invents.
    const DOCUMENTED: &[(&str, i32)] = &[
        ("AlertMessage", 0x08),
        ("Animation", 0x36),
        ("Application", 0x0E),
        ("Assistant", 0x20),
        ("BlockQuote", 0x431),
        ("Border", 0x13),
        ("ButtonDropDown", 0x38),
        ("ButtonDropGrid", 0x3A),
        ("ButtonMenu", 0x39),
        ("Canvas", 0x35),
        ("Caret", 0x07),
        ("Cell", 0x1D),
        ("Chart", 0x11),
        ("CheckBox", 0x2C),
        ("Client", 0x0A),
        ("Clock", 0x3D),
        ("ColorChooser", 0x404),
        ("Column", 0x1B),
        ("ColumnHeader", 0x19),
        ("ComboBox", 0x2E),
        ("ComplementaryContent", 0x42C),
        ("Cursor", 0x06),
        ("Desktop", 0x82),
        ("Dial", 0x31),
        ("Dialog", 0x12),
        ("Document", 0x0F),
        ("EditableText", 0x2A),
        ("Equation", 0x37),
        ("Footer", 0x40E),
        ("Form", 0x410),
        ("Graphic", 0x28),
        ("Grip", 0x04),
        ("Grouping", 0x14),
        ("Heading", 0x414),
        ("HelpBalloon", 0x1F),
        ("HotkeyField", 0x32),
        ("Indicator", 0x27),
        ("LayeredPane", 0x80),
        ("Link", 0x1E),
        ("List", 0x21),
        ("ListItem", 0x22),
        ("MenuBar", 0x02),
        ("MenuItem", 0x0C),
        ("NoRole", 0x00),
        ("Note", 0x41B),
        ("Notification", 0x86),
        ("PageTab", 0x25),
        ("PageTabList", 0x3C),
        ("Paragraph", 0x83),
        ("Pane", 0x10),
        ("PopupMenu", 0x0B),
        ("ProgressBar", 0x30),
        ("PropertyPage", 0x26),
        ("Button", 0x2B),
        ("RadioButton", 0x2D),
        ("Row", 0x1C),
        ("RowHeader", 0x1A),
        ("ScrollBar", 0x03),
        ("Section", 0x85),
        ("Separator", 0x15),
        ("Slider", 0x33),
        ("Sound", 0x05),
        ("SpinBox", 0x34),
        ("Splitter", 0x3E),
        ("StaticText", 0x29),
        ("StatusBar", 0x17),
        ("Switch", 0x87),
        ("Table", 0x18),
        ("Terminal", 0x81),
        ("TitleBar", 0x01),
        ("ToolBar", 0x16),
        ("ToolTip", 0x0D),
        ("Tree", 0x23),
        ("TreeItem", 0x24),
        ("UserRole", 0xFFFF),
        ("WebDocument", 0x84),
        ("Whitespace", 0x3B),
        ("Window", 0x09),
    ];

    #[test]
    fn no_documented_role_is_missing() {
        for (key, qt) in DOCUMENTED {
            let row = ROLES.iter().find(|r| r.key == *key);
            assert_eq!(row.map(|r| r.qt), Some(*qt), "Accessible.{key}");
        }
        for r in ROLES {
            assert!(
                DOCUMENTED.iter().any(|(key, _)| *key == r.key),
                "Accessible.{} is not a documented role",
                r.key
            );
        }
        assert_eq!(ROLES.len(), DOCUMENTED.len());
    }

    #[test]
    fn every_role_maps_or_is_one_of_the_four_that_say_nothing() {
        for r in ROLES {
            let nothing = matches!(
                r.role,
                Role::NoRole | Role::Whitespace | Role::Cursor | Role::Caret
            );
            assert_eq!(r.accesskit.is_none(), nothing, "{}", r.key);
            // No flags, no refinement.
            assert_eq!(r.role.refined(RoleFlags::default()), r.accesskit);
        }
    }

    #[test]
    fn flags_refine_the_role() {
        let f = RoleFlags::default;
        let cases = [
            (
                Role::Button,
                RoleFlags {
                    default_button: true,
                    ..f()
                },
                Ak::DefaultButton,
            ),
            (
                Role::ComboBox,
                RoleFlags {
                    editable: true,
                    ..f()
                },
                Ak::EditableComboBox,
            ),
            (
                Role::EditableText,
                RoleFlags {
                    multi_line: true,
                    ..f()
                },
                Ak::MultilineTextInput,
            ),
            (
                Role::EditableText,
                RoleFlags {
                    search_edit: true,
                    ..f()
                },
                Ak::SearchInput,
            ),
            (
                Role::EditableText,
                RoleFlags {
                    password_edit: true,
                    ..f()
                },
                Ak::PasswordInput,
            ),
            (
                Role::List,
                RoleFlags {
                    selectable: true,
                    ..f()
                },
                Ak::ListBox,
            ),
            (
                Role::ListItem,
                RoleFlags {
                    selectable: true,
                    ..f()
                },
                Ak::ListBoxOption,
            ),
            (
                Role::MenuItem,
                RoleFlags {
                    checkable: true,
                    ..f()
                },
                Ak::MenuItemCheckBox,
            ),
            (
                Role::MenuItem,
                RoleFlags {
                    checkable: true,
                    exclusive: true,
                    ..f()
                },
                Ak::MenuItemRadio,
            ),
        ];
        for (role, flags, want) in cases {
            assert_eq!(role.refined(flags), Some(want), "{role:?} {flags:?}");
        }
        // A password field that is also a search field or has several
        // lines stays a password field.
        let all = RoleFlags {
            password_edit: true,
            search_edit: true,
            multi_line: true,
            ..f()
        };
        assert_eq!(Role::EditableText.refined(all), Some(Ak::PasswordInput));
        // A flag that means nothing to a role changes nothing.
        let odd = RoleFlags {
            default_button: true,
            ..f()
        };
        assert_eq!(Role::CheckBox.refined(odd), Some(Ak::CheckBox));
        assert_eq!(Role::NoRole.refined(odd), None);
    }

    #[test]
    fn a_dial_is_a_slider_called_a_dial() {
        assert_eq!(Role::Dial.accesskit(), Some(Ak::Slider));
        assert_eq!(Role::Dial.role_description(), Some("dial"));
        assert_eq!(Role::Slider.role_description(), None);
    }

    #[test]
    fn flag_defaults_follow_qt() {
        for r in ROLES {
            let role = r.role;
            assert_eq!(
                role.checkable_by_default(),
                matches!(role, Role::CheckBox | Role::RadioButton | Role::Switch),
                "{}",
                r.key
            );
            assert_eq!(role.editable_by_default(), role == Role::EditableText);
            assert_eq!(role.read_only_by_default(), role == Role::StaticText);
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
    fn focusable_defaults_follow_qt() {
        // The documentation's list, and `StaticText`, which Qt 6.11 adds.
        let on = [
            Role::StaticText,
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
