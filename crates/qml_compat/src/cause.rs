//! Why the pass keeps a file, as a value. Each kind of reason is one
//! variant, worded once in its `Display`, so a test or a tool asks which
//! reason it is instead of matching its prose; the wording is what
//! `ljos migrate` and `--compat lower` print.

use std::fmt;

use crate::{CONTROLS, CONTROLS_STYLE, LAYOUT, MODULE};

/// Why a file is kept as it was written.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Cause {
    /// The file does not parse.
    Unparsed,
    /// What the pass made of the file does not parse.
    LoweredUnparsed,
    /// The build has no type table of a module the pass reads.
    NoDescription {
        module: &'static str,
    },
    /// The build has no type table of a controls module or of Ljós's.
    NoControlsDescription {
        uri: String,
    },
    /// `import X as Y` of a module whose names the pass would rename.
    ImportedUnderName {
        uri: String,
    },
    /// `QtQuick.Controls` where the style the program starts with is not
    /// Ljós's (`None`: it is chosen as the program starts).
    ControlsStyle {
        style: Option<String>,
    },
    /// An import of a module of the project whose types are native.
    NativeModule {
        uri: String,
    },
    /// An import of a module the pass does not lower.
    ModuleNotLowered {
        uri: String,
    },
    /// An import, beside Qt's modules, of a Ljós module the pass writes
    /// for them: the lowered file would import it twice, and its types
    /// stand for Qt's where they share a name.
    ImportsWhatItLowersTo {
        uri: String,
    },
    /// A Qt type the Ljós module it lowers to does not have yet.
    NotInModule {
        name: String,
        module: &'static str,
    },
    /// A layout's attached properties, which `Ljos.Layout` lacks.
    AttachedNotInLayout {
        name: String,
    },
    /// An object made as `Module.Type`.
    QualifiedType {
        name: String,
    },
    /// A name that a Ljós module the lowered file imports would take;
    /// `is` is whose the name is, `None` when no module the file imports
    /// has it.
    FoundFirst {
        name: String,
        is: Option<Owner>,
        module: &'static str,
    },
    /// Qt's type, which in the lowered file the project's or the program's
    /// `name`, imported after `module`'s line, would shadow.
    ShadowedInLowered {
        name: String,
        by: Owner,
        module: &'static str,
    },
    /// A name that is neither a file beside the file nor a type of the
    /// program's modules.
    NotBesideNorNative {
        name: String,
        modules: Vec<String>,
    },
    /// A member set through a `PropertyChanges`'s `target`; `what` is as
    /// written (`` `font { … }` ``, `` `Layout.fillWidth` ``).
    ThroughTarget {
        what: String,
    },
    /// A property declared under a name Ljós renames on its type.
    DeclaresRenamed {
        name: String,
    },
    /// A member set on something whose type is not known; `on` says what
    /// (`the object`, `a value source's target`).
    SetOnUnknown {
        name: String,
        on: &'static str,
    },
    /// A member set on an object whose type is not known.
    SetOnUnknownObject {
        name: String,
    },
    /// A handler of a signal of an object whose type is not known.
    HandlerOnUnknown {
        handler: String,
    },
    /// A member of a program's type that its toolkit base has under a name
    /// Ljós renames.
    NativeRenamed {
        name: String,
        native: String,
        base: String,
        new: String,
    },
    /// `grabPermissions`, which Ljós's API has as a gesture's `takeOver`.
    GrabPermissions,
    /// `Qt.createQmlObject`.
    QmlText,
    /// An attached object read off a non-item `on`.
    AttachedOffNoItem {
        name: String,
        on: String,
    },
    /// An attached object read off a program's type built on nothing of the
    /// toolkit's.
    AttachedOffNative {
        name: String,
    },
    /// An attached object read off something whose type is not known.
    AttachedOffUnknown {
        name: String,
    },
    /// A member read off something whose type is not known.
    ReadOffUnknown {
        name: String,
    },
    /// `x["name"]`, of a member Ljós renames.
    ComputedRenamed {
        key: String,
    },
    /// A free name that, lowered, would be a member of the scope or root.
    OwnOnceLowered {
        name: String,
        new: String,
        old: String,
    },
    /// A free name that may be a member of an object whose type is not
    /// known.
    MaybeMember {
        name: String,
    },
    /// A script's local called as a member Ljós renames.
    LocalCalledRenamed {
        name: String,
    },
    Font(Font),
    Loader(Loader),
    Animated(Animated),
    Row(Row),
    Layout(Layout),
    /// A file lowered alone that `with`, which is kept, uses.
    Group {
        with: String,
        cause: Box<Cause>,
    },
    /// A file a file in Ljós's API, `by`, uses, binding on it `name`: what
    /// Ljós's `ty`, which the file's root is lowered to, calls `now`.
    BoundAsQt {
        by: String,
        name: String,
        ty: String,
        now: String,
    },
    /// A script at the path of the rows module, which is the project's own.
    RowsScript {
        path: String,
    },
    /// A file found only through another, which `ljos migrate` was not
    /// named.
    NotNamed,
    /// An `XMLHttpRequest` that is not of the shape written as `fetch`.
    Xhr(Xhr),
    /// A member of `Qt` with no home in Ljós's API yet (docs/DIALECT.md
    /// §4.3.1): `member` as written after `Qt.`.
    QtNoHome {
        member: String,
    },
    /// A member of `Qt` whose home is a name the lowered file could not
    /// reach: `name`, which `by` takes there.
    QtHomeHidden {
        member: String,
        name: &'static str,
        by: Hider,
    },
    /// `Qt.resolvedUrl` given the object to resolve against, which `url`
    /// does not take.
    QtResolvedAgainst,
    /// A colour named by `Qt` (`Qt.red`) where the value is no `color`:
    /// it is a number there.
    QtColourNumber {
        name: String,
    },
    /// `Qt.formatDate` and its family given a format that is not written as
    /// a string, or a locale: a locale's `formatDate` reads a number as
    /// another enum.
    QtFormatNotText {
        member: String,
    },
    /// `Qt.application` handed on whole, as a target or a value: `App` is
    /// the application read member by member, not the object.
    QtApplicationWhole,
    /// `Qt.exit` given a code not written as 0, which `App.exit` does not
    /// carry to the process yet.
    QtExitCode,
    /// A type a script names whose name in Ljós's API is a global of
    /// JavaScript's there (`QtObject`, which is `Object`).
    RenamedToGlobal {
        name: String,
        new: &'static str,
    },
}

/// What takes a name a member of `Qt` lowers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Hider {
    /// A property, a function or a variable of the file of that name.
    Own,
    /// A member of the type of one of the file's objects.
    Member,
    /// A role of a delegate's model.
    Role,
    /// An object of a type the pass does not know, which may have it.
    UnknownType,
    /// A type of a directory or module the file imports by path.
    Directory,
    /// A file beside the file, or what the root of one declares.
    Beside,
    /// Another word of the file: a local, a parameter, a type it names.
    Word,
}

impl Hider {
    /// What takes the name, as a keep reason says it after the name.
    fn which(self) -> &'static str {
        match self {
            Hider::Own => "which a property of the file hides",
            Hider::Member => "which a member of an object's type hides",
            Hider::Role => "which a role of a delegate could hide",
            Hider::UnknownType => "which an object of a type the pass does not know could have",
            Hider::Directory => "which a type of a directory the file imports could be",
            Hider::Beside => "which a file beside it has",
            Hider::Word => "which the file names something else",
        }
    }
}

/// Whose a name is, as a keep reason says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// One of the program's own types.
    Program,
    /// A file of the project.
    Project,
}

impl Owner {
    fn is(self) -> &'static str {
        match self {
            Owner::Program => "is one of the program's own types",
            Owner::Project => "is a file of the project",
        }
    }

    fn whose(self) -> &'static str {
        match self {
            Owner::Program => "the program's",
            Owner::Project => "the project's",
        }
    }
}

/// Why a font's member keeps a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Font {
    /// `.pointSize` read or written in a script: Ljós's font has only a
    /// size in pixels.
    PointSize,
    /// `font.pointSize` beside `font.pixelSize`.
    PointAndPixelSize,
    /// `font.pointSize` given by a block.
    PointSizeBlock,
    /// `font.pointSize`, whose lowering names `Screen`, which `by` takes.
    ScreenHidden { by: Hider },
    /// `font.bold` beside `font.weight`.
    BoldAndWeight,
    /// `font.bold` given by a block.
    BoldBlock,
    /// `.bold` read or written in a script.
    BoldRead,
}

/// Why a `Loader` keeps a file.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Loader {
    /// `source` is computed.
    SourceMade,
    /// `source` is no file beside this one.
    SourceNotBeside { file: String },
    /// `source`'s name is not a type's.
    SourceNotType { file: String },
    /// Both `source` and `sourceComponent`.
    Both,
    /// `source` is read or set elsewhere.
    SourceRead,
    /// `sourceComponent` is read or heard.
    ComponentRead,
}

/// Why an animation's `property` or `properties` keeps a file: the names
/// it holds as strings are members (docs/DIALECT.md R7), renamed as the
/// objects it animates rename them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Animated {
    /// `property` or `properties` (`binding`) is not a string written out.
    Made { binding: String },
    /// `name` is a member Ljós renames, and what the animation moves is
    /// not known.
    Unknown { name: String },
    /// `name` is `new` on some of what the animation moves and keeps its
    /// name on others.
    Split { name: String, new: String },
}

/// Why what a delegate reads of its model's row keeps a file.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Row {
    /// `what` would be written `name`, a member of an object here.
    Taken { what: String, name: &'static str },
    /// `what` reads `part` of a row the file does not show; the root (or,
    /// `in_component`, the `Component`'s root) should declare `declare`.
    Unshown {
        what: String,
        part: &'static str,
        declare: String,
        in_component: bool,
    },
    /// A role the model does not have.
    MissingRole { what: String },
    /// A role of an array.
    RoleOfArray { what: String, member: String },
    /// A role of a model of values.
    RoleOfValues { what: String },
    /// A role of a model the file does not show.
    RoleOfUnshown { what: String, member: String },
    /// `model.member` where `model` may be an object's.
    ModelMemberUnknown { member: String },
    /// `model` may be a member of an object whose type is not known.
    ModelMaybeMember,
    /// `model` of a model whose roles the file does not show.
    ModelRolesUnshown,
    /// A local called `model`.
    LocalModel,
    /// A local called as the row's `name`.
    LocalRow { name: String },
}

/// Why a positioner or a layout keeps a file (`rules/layout/arrange.rs`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Layout {
    /// A member of Qt's positioner `qt` that Ljós's has not.
    PositionerOnly { name: String, qt: &'static str },
    /// A member that keeps a `Flow` as Qt's.
    FlowOnly { name: String },
    /// A name only Qt's positioners have.
    PositionersUse { name: String },
    /// A child's `Layout.*`, ignored by Qt's positioner; `root` is the
    /// type whose root binds it, when the child does not.
    Ignored {
        name: String,
        root: Option<String>,
        qt: &'static str,
    },
    /// A child's cross coordinate `name`, which Qt's positioner leaves.
    Crossed {
        name: String,
        root: Option<String>,
        qt: &'static str,
    },
    /// A child's `anchors.*`.
    Anchored {
        name: String,
        qt: &'static str,
        ljos: &'static str,
    },
    /// A positioner's child's anchor across it, which Qt's positioner
    /// honours.
    AnchoredAcross {
        name: String,
        qt: &'static str,
        ljos: &'static str,
    },
    /// A member Ljós's `ljos` lacks.
    NotYet { name: String, ljos: &'static str },
    /// `rows` or `columns` of a `Grid` that is not a whole number.
    GridCount { name: String },
    /// Where Qt's `Grid` differs from Ljós's; `for_some`: for some of the
    /// numbers of children it may show.
    Grid { differs: Differs, for_some: bool },
    /// How many children a `Grid` shows is not known.
    GridShown { why: String },
    /// A `Grid`'s `name` that is not one the pass can read.
    GridUnread { name: String },
    /// The count a `GridLayout` ignores along its flow.
    GridLayoutIgnored {
        name: &'static str,
        flow: &'static str,
    },
    /// Room no child fills; `fill` is what a child should be given.
    Spreads {
        qt: &'static str,
        ljos: &'static str,
        root: bool,
        fill: &'static str,
    },
    /// A `GridLayout` child's cell.
    Cell { name: String },
    /// A layout in a layout.
    Fills {
        name: String,
        qt: &'static str,
        ljos: &'static str,
    },
    /// A size that is not a whole number.
    NotWhole {
        what: String,
        qt: &'static str,
        ljos: &'static str,
    },
    /// A child's size, which is not its size in Qt's layout.
    NotSize {
        name: String,
        qt: &'static str,
        ljos: &'static str,
        size: &'static str,
        preferred: &'static str,
    },
    /// `Layout.alignment` that is not an expression.
    AlignmentNoExpression,
    /// `Layout.alignment` the pass cannot read.
    AlignmentUnread {
        qt: &'static str,
        ljos: &'static str,
    },
    /// A file of the project whose size is not known.
    FileSize { name: String, qt: &'static str },
    /// Children given through `children` or `data`.
    ChildrenThrough { qt: &'static str, through: String },
    /// A `Repeater`'s delegate the pass cannot read.
    RepeaterDelegate,
}

/// How Qt's `Grid` places children where Ljós's does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Differs {
    /// A child beyond its rows and columns.
    Beyond,
    /// The spacing after a line it does not fill.
    SpacingAfter { line: &'static str },
    /// The spacing of a line it leaves empty.
    EmptyLine {
        line: &'static str,
        lines: usize,
        children: usize,
        per_line: usize,
    },
}

impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Cause::Unparsed => f.write_str("it does not parse"),
            Cause::LoweredUnparsed => f.write_str("the lowered text does not parse"),
            Cause::NoDescription { module } => {
                write!(f, "this build has no description of {module}")
            }
            Cause::NoControlsDescription { uri } => {
                write!(f, "this build has no description of `{uri}` or {CONTROLS}")
            }
            Cause::ImportedUnderName { uri } => write!(f, "it imports `{uri}` under a name"),
            Cause::ControlsStyle { style: Some(style) } => write!(
                f,
                "it imports `QtQuick.Controls`, which is the {style} style where it runs, and {CONTROLS} is {CONTROLS_STYLE}"
            ),
            Cause::ControlsStyle { style: None } => f.write_str(
                "it imports `QtQuick.Controls`, whose style is chosen as the program starts",
            ),
            Cause::NativeModule { uri } => write!(
                f,
                "it imports `{uri}`, a module of the project whose types are native: they are \
                 not lowered"
            ),
            Cause::ModuleNotLowered { uri } => {
                write!(f, "it imports `{uri}`, which is not lowered yet")
            }
            Cause::ImportsWhatItLowersTo { uri } => write!(
                f,
                "it imports `{uri}` beside Qt's modules, which lower to it"
            ),
            Cause::NotInModule { name, module } => write!(f, "`{name}` is not in {module} yet"),
            Cause::AttachedNotInLayout { name } => {
                write!(f, "`{name}`'s attached properties are not in {LAYOUT} yet")
            }
            Cause::QualifiedType { name } => {
                write!(f, "`{name}` is a type of a module named in the file")
            }
            Cause::FoundFirst { name, is, module } => write!(
                f,
                "`{name}` {}, and {module}, which the lowered file imports, has a `{name}` that would be found first",
                match is {
                    Some(owner) => owner.is(),
                    None => "is not a type of the modules the file imports",
                }
            ),
            Cause::ShadowedInLowered { name, by, module } => write!(
                f,
                "`{name}` is Qt's, but in the lowered file {} `{name}`, imported after the line {module} takes, would be found first",
                by.whose()
            ),
            Cause::NotBesideNorNative { name, modules } => write!(
                f,
                "`{name}` is neither a file beside it nor a type of `{}` as the program describes it",
                modules.join("`, `")
            ),
            Cause::ThroughTarget { what } => write!(
                f,
                "{what} is set through the target of a `PropertyChanges`, which Ljós's API names \
                 in each change"
            ),
            Cause::DeclaresRenamed { name } => {
                write!(
                    f,
                    "it declares `{name}` on a type whose `{name}` Ljós renames"
                )
            }
            Cause::SetOnUnknown { name, on } => {
                write!(f, "`{name}` is set on {on}, whose type is not known")
            }
            Cause::SetOnUnknownObject { name } => {
                write!(f, "`{name}` is set on an object whose type is not known")
            }
            Cause::HandlerOnUnknown { handler } => write!(
                f,
                "`{handler}` handles a signal of an object whose type is not known"
            ),
            Cause::NativeRenamed {
                name,
                native,
                base,
                new,
            } => write!(
                f,
                "`{name}` of `{native}` is {base}'s, which {MODULE} calls `{new}`, and the program's type has it as `{name}`"
            ),
            Cause::GrabPermissions => f.write_str(
                "`grabPermissions` is a gesture's `takeOver` in Ljós's API, which is not built yet",
            ),
            Cause::QmlText => {
                f.write_str("it makes objects from QML text, which is not lowered yet")
            }
            Cause::AttachedOffNoItem { name, on } => write!(
                f,
                "`.{name}` is read off a `{on}`, which is no item, and only an item's is Qt's attached `{name}`"
            ),
            Cause::AttachedOffNative { name } => write!(
                f,
                "`.{name}` is read off one of the program's own types, built on no type of the toolkit's, and only an item's is Qt's attached `{name}`"
            ),
            Cause::AttachedOffUnknown { name } => write!(
                f,
                "`.{name}` is read off something whose type is not known, and only an item's is Qt's attached `{name}`"
            ),
            Cause::ReadOffUnknown { name } => {
                write!(f, "`.{name}` is read off something whose type is not known")
            }
            Cause::ComputedRenamed { key } => {
                write!(f, "`[\"{key}\"]` names a member Ljós renames")
            }
            Cause::OwnOnceLowered { name, new, old } => write!(
                f,
                "`{name}` would be the `{new}`'s own once lowered (Qt's `{old}`), not what it names in Qt"
            ),
            Cause::MaybeMember { name } => write!(
                f,
                "`{name}` may be a member of an object whose type is not known"
            ),
            Cause::LocalCalledRenamed { name } => {
                write!(f, "a local is called `{name}`, as a member Ljós renames is")
            }
            Cause::Font(font) => font.fmt(f),
            Cause::Loader(loader) => loader.fmt(f),
            Cause::Animated(animated) => animated.fmt(f),
            Cause::Row(row) => row.fmt(f),
            Cause::Layout(layout) => layout.fmt(f),
            Cause::Group { with, cause } => {
                write!(f, "it is used by `{with}.qml`, which is kept: {cause}")
            }
            Cause::BoundAsQt { by, name, ty, now } => write!(
                f,
                "`{by}.qml`, in Ljós's API, binds `{name}` on it, which Ljós's `{ty}` calls `{now}`: rename it there as this file is lowered"
            ),
            Cause::RowsScript { path } => write!(
                f,
                "`{path}` is a script of the project's, not the module of this file's rows"
            ),
            Cause::NotNamed => f.write_str("it is not named: name it too"),
            Cause::Xhr(xhr) => xhr.fmt(f),
            Cause::QtNoHome { member } => {
                write!(f, "`Qt.{member}` has no home in Ljós's API yet")
            }
            Cause::QtHomeHidden { member, name, by } => {
                write!(f, "`Qt.{member}` needs `{name}`, {}", by.which())
            }
            Cause::QtResolvedAgainst => f.write_str(
                "`Qt.resolvedUrl` given an object to resolve against has no home in Ljós's API yet",
            ),
            Cause::QtColourNumber { name } => write!(
                f,
                "`Qt.{name}` is a number here: Ljós's API names a colour only where it goes to a `color`"
            ),
            Cause::QtFormatNotText { member } => write!(
                f,
                "`Qt.{member}` given a format that is not a string has no home in Ljós's API yet"
            ),
            Cause::QtApplicationWhole => f.write_str(
                "`Qt.application` handed on whole has no home in Ljós's API yet: `App` is read member by member",
            ),
            Cause::QtExitCode => f.write_str(
                "`Qt.exit` with a code other than 0 has no home in Ljós's API yet: `App.exit` does not carry the code to the process",
            ),
            Cause::RenamedToGlobal { name, new } => write!(
                f,
                "`{name}` is `{new}` in Ljós's API, which a script reads as JavaScript's"
            ),
        }
    }
}

impl fmt::Display for Font {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Font::PointSize => f.write_str(
                "`.pointSize` of a font is read or written, and Ljós's font has only a `size` in pixels",
            ),
            Font::PointAndPixelSize => f.write_str(
                "`font.pointSize` and `font.pixelSize` are both set, and are one `size` in Ljós",
            ),
            Font::PointSizeBlock => {
                f.write_str("`font.pointSize` is given by a block, which is not lowered yet")
            }
            Font::ScreenHidden { by } => {
                write!(f, "`font.pointSize` needs `Screen`, {}", by.which())
            }
            Font::BoldAndWeight => f.write_str(
                "`font.bold` and `font.weight` are both set, and are one `weight` in Ljós",
            ),
            Font::BoldBlock => {
                f.write_str("`font.bold` is given by a block, which is not lowered yet")
            }
            Font::BoldRead => f.write_str(
                "`.bold` of a font is read or written, and Ljós's font has only a `weight`",
            ),
        }
    }
}

impl fmt::Display for Loader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Loader::SourceMade => f.write_str(
                "a `Loader`'s `source` is made as it runs, so what it loads is not known",
            ),
            Loader::SourceNotBeside { file } => write!(
                f,
                "a `Loader`'s `source` is \"{file}\", which is not a file beside this one"
            ),
            Loader::SourceNotType { file } => write!(
                f,
                "a `Loader`'s `source` is \"{file}\", whose name is not a type's"
            ),
            Loader::Both => {
                f.write_str("a `Loader` is given both a `source` and a `sourceComponent`")
            }
            Loader::SourceRead => {
                f.write_str("a `Loader`'s `source` is read or set as well as written")
            }
            Loader::ComponentRead => f.write_str(
                "a `Loader`'s `sourceComponent` is read or heard, and lowered it is not empty",
            ),
        }
    }
}

impl fmt::Display for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Row::Taken { what, name } => write!(
                f,
                "`{what}` would be `{name}`, which an object here has a member of"
            ),
            Row::Unshown {
                what,
                part,
                declare,
                in_component: true,
            } => write!(
                f,
                "`{what}` reads {part} of a view the `Component` it is in is not written in the `delegate:` of, or given to by id alone; give the Component's root `required property {declare}`"
            ),
            Row::Unshown {
                what,
                part,
                declare,
                in_component: false,
            } => write!(
                f,
                "`{what}` reads {part} of a view the file is not shown in; give the root `required property {declare}`"
            ),
            Row::MissingRole { what } => write!(
                f,
                "`{what}` reads a role the model does not have, which is `undefined`; take it out"
            ),
            Row::RoleOfArray { what, member } => write!(
                f,
                "`{what}` reads a role of an array, which gives none; read `modelData.{member}`, the element's"
            ),
            Row::RoleOfValues { what } => write!(
                f,
                "`{what}` reads a role of a model of values, which gives none; take it out"
            ),
            Row::RoleOfUnshown { what, member } => write!(
                f,
                "`{what}` reads a role of a model the file does not show, which `modelData.{member}` reads only of a model of several roles; give the delegate `required property var {member}`"
            ),
            Row::ModelMemberUnknown { member } => write!(
                f,
                "`model.{member}` may read a member of an object whose type is not known"
            ),
            Row::ModelMaybeMember => {
                f.write_str("`model` may be a member of an object whose type is not known")
            }
            Row::ModelRolesUnshown => f.write_str(
                "`model` is the row of a model whose roles the file does not show; read the roles it needs by name",
            ),
            Row::LocalModel => f.write_str("a local is called `model`, as a delegate's row is"),
            Row::LocalRow { name } => {
                write!(f, "a local is called `{name}`, as the row's `{name}` is")
            }
        }
    }
}

impl fmt::Display for Layout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Layout::PositionerOnly { name, qt } => {
                write!(f, "`{name}` is a member of Qt's `{qt}` that Ljós's has not")
            }
            Layout::FlowOnly { name } => write!(
                f,
                "`{name}` keeps a `Flow` as Qt's until the Box rules reach a wrapping line"
            ),
            Layout::PositionersUse { name } => {
                write!(f, "it uses `{name}`, which only Qt's positioners have")
            }
            Layout::Ignored {
                name,
                root: None,
                qt,
            } => write!(
                f,
                "`{name}` is ignored by Qt's `{qt}` and would be honoured by Ljós's"
            ),
            Layout::Ignored {
                name,
                root: Some(root),
                qt,
            } => write!(
                f,
                "`{root}`'s root sets `{name}`, which is ignored by Qt's `{qt}` and would be \
                 honoured by Ljós's"
            ),
            Layout::Crossed {
                name,
                root: None,
                qt,
            } => write!(
                f,
                "Qt's `{qt}` leaves a child's `{name}` to it, and Ljós's sets it"
            ),
            Layout::Crossed {
                name,
                root: Some(root),
                qt,
            } => write!(
                f,
                "`{root}`'s root sets `{name}`, which Qt's `{qt}` leaves to it, and Ljós's sets"
            ),
            Layout::Anchored { name, qt, ljos } => write!(
                f,
                "`{name}` on a child of a `{qt}` is overridden by it, and is an error in Ljós's `{ljos}`"
            ),
            Layout::AnchoredAcross { name, qt, ljos } => write!(
                f,
                "`{name}` places a child across Qt's `{qt}`, which honours it; Ljós's `{ljos}` places it across with `Layout.alignment`"
            ),
            Layout::NotYet { name, ljos } => write!(f, "`{name}` is not in Ljós's `{ljos}` yet"),
            Layout::GridCount { name } => write!(
                f,
                "`{name}` of a `Grid` is not a whole number the pass can read, so it cannot tell \
                 where Qt's `Grid` counts room Ljós's does not"
            ),
            Layout::Grid { differs, for_some } => {
                differs.fmt(f)?;
                if *for_some {
                    f.write_str(", for as many children as it may show")?;
                }
                Ok(())
            }
            Layout::GridShown { why } => write!(
                f,
                "{why}, and how many children Qt's `Grid` places tells where it counts room \
                 Ljós's `Grid` does not"
            ),
            Layout::GridUnread { name } => {
                write!(f, "`{name}` of a `Grid` is not one the pass can read")
            }
            Layout::GridLayoutIgnored { name, flow } => write!(
                f,
                "`{name}` of a `GridLayout` is ignored along a {flow} flow, and Ljós's `Grid` \
                 would not ignore it"
            ),
            Layout::Spreads {
                qt,
                ljos,
                root,
                fill,
            } => {
                let sized_by = if *root {
                    " (a file that uses it may size it)"
                } else {
                    ""
                };
                write!(
                    f,
                    "`{qt}` spreads the room no child fills over its children, and Ljós's \
                     `{ljos}` leaves it at the end{sized_by}; give a child `{fill}: true`"
                )
            }
            Layout::Cell { name } => write!(
                f,
                "`{name}` puts a child of a `GridLayout` where the pass does not yet follow Qt"
            ),
            Layout::Fills { name, qt, ljos } => write!(
                f,
                "a `{name}` in a `{qt}` grows only where a child of it fills and is bounded by \
                 its children, and Ljós's `{ljos}` reads what this one is given otherwise"
            ),
            Layout::NotWhole { what, qt, ljos } => write!(
                f,
                "`{what}` is not a whole number the pass can read; `{qt}` rounds a child's size \
                 up and Ljós's `{ljos}` does not"
            ),
            Layout::NotSize {
                name,
                qt,
                ljos,
                size,
                preferred,
            } => write!(
                f,
                "`{size}` of a `{name}` in a `{qt}` is not its size there, and would be in \
                 Ljós's `{ljos}`; give it `{preferred}`"
            ),
            Layout::AlignmentNoExpression => f.write_str("`Layout.alignment` is not an expression"),
            Layout::AlignmentUnread { qt, ljos } => write!(
                f,
                "`Layout.alignment` is not one the pass can read, and `{qt}` centres a child that \
                 says nothing of how it is aligned across the row, where Ljós's `{ljos}` puts it \
                 at the top"
            ),
            Layout::FileSize { name, qt } => write!(
                f,
                "`{name}` is a file of the project, whose size in a `{qt}` the pass does not know"
            ),
            Layout::ChildrenThrough { qt, through } => {
                write!(f, "the children of a `{qt}` are given through `{through}`")
            }
            Layout::RepeaterDelegate => {
                f.write_str("the delegate of a `Repeater` is not one the pass can read")
            }
        }
    }
}

impl fmt::Display for Differs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Differs::Beyond => f.write_str(
                "Qt's `Grid` leaves a child beyond its rows and columns where it is, and Ljós's \
                 `Grid` places it",
            ),
            Differs::SpacingAfter { line } => write!(
                f,
                "Qt's `Grid` counts the spacing after a {line} of children it does not fill, and \
                 Ljós's `Grid` does not"
            ),
            Differs::EmptyLine {
                line,
                lines,
                children,
                per_line,
            } => write!(
                f,
                "Qt's `Grid` counts the spacing of a {line} it leaves empty, and Ljós's `Grid` \
                 does not; it has {lines} {line}s and {children} children to fill them, \
                 {per_line} to a {line}"
            ),
        }
    }
}

impl From<Font> for Cause {
    fn from(font: Font) -> Self {
        Cause::Font(font)
    }
}

impl fmt::Display for Animated {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Animated::Made { binding } => write!(
                f,
                "an animation's `{binding}` is made as it runs, so the members it names are not known"
            ),
            Animated::Unknown { name } => write!(
                f,
                "an animation's \"{name}\" names a member Ljós renames, on objects whose type is not known"
            ),
            Animated::Split { name, new } => write!(
                f,
                "an animation's \"{name}\" is `{new}` on some of what it moves and `{name}` on the rest"
            ),
        }
    }
}

impl From<Animated> for Cause {
    fn from(animated: Animated) -> Self {
        Cause::Animated(animated)
    }
}

impl From<Loader> for Cause {
    fn from(loader: Loader) -> Self {
        Cause::Loader(loader)
    }
}

impl From<Row> for Cause {
    fn from(row: Row) -> Self {
        Cause::Row(row)
    }
}

impl From<Layout> for Cause {
    fn from(layout: Layout) -> Self {
        Cause::Layout(layout)
    }
}

/// Why an `XMLHttpRequest` is not written as `fetch`
/// (`rules::network::request`): what of it has no `fetch` of its usual
/// shape.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Xhr {
    /// `open`'s third argument is `false`: the script waits for the reply.
    Waits,
    /// A member of the request's, set, read or called, that its usual shape
    /// does not have.
    Member { name: String },
    /// `readyState` read other than to wait for the request to be done.
    ReadyState,
    /// The request is a value given to something else.
    HandedOn,
    /// The handler is a `function` that reads `this`.
    This,
    /// A statement that is not the request's between its first and its
    /// `send`.
    Between,
    /// It is not opened, answered in `onreadystatechange` and sent, each
    /// once.
    Unfinished,
    /// Its `onreadystatechange` does not wait for `readyState` 4 first.
    Unguarded,
    /// The handler is not a function of no parameters written there.
    Handler,
    /// Made where it is not a local of its own (`[new XMLHttpRequest()]`).
    Made,
    /// What it is sent with spans lines, which a lowered file cannot gather
    /// onto one.
    Lines,
    /// A name the pass renames is in what it is sent with.
    Moved,
}

impl fmt::Display for Xhr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Xhr::Waits => f.write_str(
                "the request waits for its reply (`open`'s third argument is `false`), and \
                 `fetch` does not: make the function async and await `fetch`",
            ),
            Xhr::Member { name } => write!(
                f,
                "the request's `{name}` has no `fetch` of its usual shape: write it with `fetch`"
            ),
            Xhr::ReadyState => f.write_str(
                "the request's `readyState` is read where `fetch` has no such thing: write it \
                 with `fetch`",
            ),
            Xhr::HandedOn => f.write_str(
                "the request is handed on, and what is done with it is not known: write it \
                 with `fetch`",
            ),
            Xhr::This => f.write_str(
                "the handler reads `this`, which an arrow function does not have: name the \
                 request instead",
            ),
            Xhr::Between => f.write_str(
                "code runs between the request's `open` and its `send`, which `fetch` is one \
                 call for: write it with `fetch`",
            ),
            Xhr::Unfinished => f.write_str(
                "the request is not opened, answered in `onreadystatechange` and sent, each \
                 once: write it with `fetch`",
            ),
            Xhr::Unguarded => f.write_str(
                "the request's `onreadystatechange` runs as the request goes, not once it is \
                 done (`readyState` 4): write it with `fetch`",
            ),
            Xhr::Handler => f.write_str(
                "the request's `onreadystatechange` is not a function of its own: write it \
                 with `fetch`",
            ),
            Xhr::Made => f.write_str(
                "an `XMLHttpRequest` is made where it is not a local of its own: write it \
                 with `fetch`",
            ),
            Xhr::Lines => f.write_str(
                "what the request is sent with spans lines, and a lowered file keeps every \
                 line: write it with `fetch`",
            ),
            Xhr::Moved => f.write_str(
                "what the request is sent with reads a name Ljós renames: write it with `fetch`",
            ),
        }
    }
}

impl From<Xhr> for Cause {
    fn from(xhr: Xhr) -> Self {
        Cause::Xhr(xhr)
    }
}
