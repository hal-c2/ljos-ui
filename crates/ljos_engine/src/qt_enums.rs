//! The enums of the `Qt` namespace, by their names.
//!
//! A script reads a key off `Qt` without saying which enum has it
//! (`Qt.AlignLeft`), but a property says which enum it holds: the alignment
//! of a layout's cell takes `"AlignLeft"` and not `"LeftButton"`. So the
//! keys are kept by enum here, and [`crate::js`] gives every one of them to
//! `Qt`.
//!
//! The names are the ones Qt's own description of its types lists for the
//! namespace, and each number was read off a running `qml6` (6.11.2), which
//! is also why a key with the top bit set is negative: QML reads these as
//! 32-bit ints. `Key` is in [`crate::qt_keys`]; the two enums that only
//! widgets use (`WidgetAttribute`, `ApplicationAttribute`) are left out.

/// One enum of the namespace.
pub(crate) struct QtEnum {
    /// What a property is declared with: `Alignment`, `LayoutDirection`.
    pub name: &'static str,
    /// For flags, the name of one key's enum (`AlignmentFlag`); else empty.
    pub alias: &'static str,
    pub keys: &'static [(&'static str, i64)],
}

/// The enum `name` of the namespace, by either of its names.
pub(crate) fn qt_enum(name: &str) -> Option<&'static QtEnum> {
    QT_ENUMS
        .iter()
        .find(|e| e.name == name || (!e.alias.is_empty() && e.alias == name))
}

/// The number of `Qt.<key>`, for a key of any enum here.
pub(crate) fn key(key: &str) -> Option<i64> {
    QT_ENUMS
        .iter()
        .flat_map(|e| e.keys.iter())
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

pub(crate) const QT_ENUMS: &[QtEnum] = &[
    QtEnum {
        name: "GlobalColor",
        alias: "",
        keys: &[
            ("color0", 0),
            ("color1", 1),
            ("black", 2),
            ("white", 3),
            ("darkGray", 4),
            ("gray", 5),
            ("lightGray", 6),
            ("red", 7),
            ("green", 8),
            ("blue", 9),
            ("cyan", 10),
            ("magenta", 11),
            ("yellow", 12),
            ("darkRed", 13),
            ("darkGreen", 14),
            ("darkBlue", 15),
            ("darkCyan", 16),
            ("darkMagenta", 17),
            ("darkYellow", 18),
            ("transparent", 19),
        ],
    },
    QtEnum {
        name: "ColorScheme",
        alias: "",
        keys: &[("Unknown", 0), ("Light", 1), ("Dark", 2)],
    },
    QtEnum {
        name: "ContrastPreference",
        alias: "",
        keys: &[("NoPreference", 0), ("HighContrast", 1)],
    },
    QtEnum {
        name: "MouseButtons",
        alias: "MouseButton",
        keys: &[
            ("NoButton", 0),
            ("LeftButton", 1),
            ("RightButton", 2),
            ("MiddleButton", 4),
            ("BackButton", 8),
            ("XButton1", 8),
            ("ExtraButton1", 8),
            ("ForwardButton", 16),
            ("XButton2", 16),
            ("ExtraButton2", 16),
            ("TaskButton", 32),
            ("ExtraButton3", 32),
            ("ExtraButton4", 64),
            ("ExtraButton5", 128),
            ("ExtraButton6", 256),
            ("ExtraButton7", 512),
            ("ExtraButton8", 1024),
            ("ExtraButton9", 2048),
            ("ExtraButton10", 4096),
            ("ExtraButton11", 8192),
            ("ExtraButton12", 16384),
            ("ExtraButton13", 32768),
            ("ExtraButton14", 65536),
            ("ExtraButton15", 131072),
            ("ExtraButton16", 262144),
            ("ExtraButton17", 524288),
            ("ExtraButton18", 1048576),
            ("ExtraButton19", 2097152),
            ("ExtraButton20", 4194304),
            ("ExtraButton21", 8388608),
            ("ExtraButton22", 16777216),
            ("ExtraButton23", 33554432),
            ("ExtraButton24", 67108864),
            ("AllButtons", 134217727),
            ("MaxMouseButton", 67108864),
            ("MouseButtonMask", -1),
        ],
    },
    QtEnum {
        name: "Orientations",
        alias: "Orientation",
        keys: &[("Horizontal", 1), ("Vertical", 2)],
    },
    QtEnum {
        name: "FocusPolicy",
        alias: "",
        keys: &[
            ("NoFocus", 0),
            ("TabFocus", 1),
            ("ClickFocus", 2),
            ("StrongFocus", 11),
            ("WheelFocus", 15),
        ],
    },
    QtEnum {
        name: "TabFocusBehavior",
        alias: "",
        keys: &[
            ("NoTabFocus", 0),
            ("TabFocusTextControls", 1),
            ("TabFocusListControls", 2),
            ("TabFocusAllControls", 255),
        ],
    },
    QtEnum {
        name: "SortOrder",
        alias: "",
        keys: &[("AscendingOrder", 0), ("DescendingOrder", 1)],
    },
    QtEnum {
        name: "SplitBehavior",
        alias: "SplitBehaviorFlags",
        keys: &[("KeepEmptyParts", 0), ("SkipEmptyParts", 1)],
    },
    QtEnum {
        name: "Alignment",
        alias: "AlignmentFlag",
        keys: &[
            ("AlignLeft", 1),
            ("AlignLeading", 1),
            ("AlignRight", 2),
            ("AlignTrailing", 2),
            ("AlignHCenter", 4),
            ("AlignJustify", 8),
            ("AlignAbsolute", 16),
            ("AlignHorizontal_Mask", 31),
            ("AlignTop", 32),
            ("AlignBottom", 64),
            ("AlignVCenter", 128),
            ("AlignBaseline", 256),
            ("AlignVertical_Mask", 480),
            ("AlignCenter", 132),
        ],
    },
    QtEnum {
        name: "TextFlag",
        alias: "",
        keys: &[
            ("TextSingleLine", 256),
            ("TextDontClip", 512),
            ("TextExpandTabs", 1024),
            ("TextShowMnemonic", 2048),
            ("TextWordWrap", 4096),
            ("TextWrapAnywhere", 8192),
            ("TextDontPrint", 16384),
            ("TextIncludeTrailingSpaces", 134217728),
            ("TextHideMnemonic", 32768),
            ("TextJustificationForced", 65536),
            ("TextForceLeftToRight", 131072),
            ("TextForceRightToLeft", 262144),
            ("TextLongestVariant", 524288),
        ],
    },
    QtEnum {
        name: "TextElideMode",
        alias: "",
        keys: &[
            ("ElideLeft", 0),
            ("ElideRight", 1),
            ("ElideMiddle", 2),
            ("ElideNone", 3),
        ],
    },
    QtEnum {
        name: "WindowFlags",
        alias: "WindowType",
        keys: &[
            ("Widget", 0),
            ("Window", 1),
            ("Dialog", 3),
            ("Sheet", 5),
            ("Drawer", 7),
            ("Popup", 9),
            ("Tool", 11),
            ("ToolTip", 13),
            ("SplashScreen", 15),
            ("Desktop", 17),
            ("SubWindow", 18),
            ("ForeignWindow", 33),
            ("CoverWindow", 65),
            ("WindowType_Mask", 255),
            ("MSWindowsFixedSizeDialogHint", 256),
            ("MSWindowsOwnDC", 512),
            ("BypassWindowManagerHint", 1024),
            ("X11BypassWindowManagerHint", 1024),
            ("FramelessWindowHint", 2048),
            ("WindowTitleHint", 4096),
            ("WindowSystemMenuHint", 8192),
            ("WindowMinimizeButtonHint", 16384),
            ("WindowMaximizeButtonHint", 32768),
            ("WindowMinMaxButtonsHint", 49152),
            ("WindowContextHelpButtonHint", 65536),
            ("WindowShadeButtonHint", 131072),
            ("WindowStaysOnTopHint", 262144),
            ("WindowTransparentForInput", 524288),
            ("WindowOverridesSystemGestures", 1048576),
            ("WindowDoesNotAcceptFocus", 2097152),
            ("MaximizeUsingFullscreenGeometryHint", 4194304),
            ("ExpandedClientAreaHint", 4194304),
            ("NoTitleBarBackgroundHint", 8388608),
            ("CustomizeWindowHint", 33554432),
            ("WindowStaysOnBottomHint", 67108864),
            ("WindowCloseButtonHint", 134217728),
            ("MacWindowToolBarButtonHint", 268435456),
            ("BypassGraphicsProxyWidget", 536870912),
            ("NoDropShadowWindowHint", 1073741824),
            ("WindowFullscreenButtonHint", -2147483648),
        ],
    },
    QtEnum {
        name: "WindowStates",
        alias: "WindowState",
        keys: &[
            ("WindowNoState", 0),
            ("WindowMinimized", 1),
            ("WindowMaximized", 2),
            ("WindowFullScreen", 4),
            ("WindowActive", 8),
        ],
    },
    QtEnum {
        name: "ApplicationState",
        alias: "",
        keys: &[
            ("ApplicationSuspended", 0),
            ("ApplicationHidden", 1),
            ("ApplicationInactive", 2),
            ("ApplicationActive", 4),
        ],
    },
    QtEnum {
        name: "ScreenOrientations",
        alias: "ScreenOrientation",
        keys: &[
            ("PrimaryOrientation", 0),
            ("PortraitOrientation", 1),
            ("LandscapeOrientation", 2),
            ("InvertedPortraitOrientation", 4),
            ("InvertedLandscapeOrientation", 8),
        ],
    },
    QtEnum {
        name: "ImageConversionFlags",
        alias: "ImageConversionFlag",
        keys: &[
            ("ColorMode_Mask", 3),
            ("AutoColor", 0),
            ("ColorOnly", 3),
            ("MonoOnly", 2),
            ("AlphaDither_Mask", 12),
            ("ThresholdAlphaDither", 0),
            ("OrderedAlphaDither", 4),
            ("DiffuseAlphaDither", 8),
            ("NoAlpha", 12),
            ("Dither_Mask", 48),
            ("DiffuseDither", 0),
            ("OrderedDither", 16),
            ("ThresholdDither", 32),
            ("DitherMode_Mask", 192),
            ("AutoDither", 0),
            ("PreferDither", 64),
            ("AvoidDither", 128),
            ("NoOpaqueDetection", 256),
            ("NoFormatConversion", 512),
        ],
    },
    QtEnum {
        name: "BGMode",
        alias: "",
        keys: &[("TransparentMode", 0), ("OpaqueMode", 1)],
    },
    QtEnum {
        name: "KeyboardModifiers",
        alias: "KeyboardModifier",
        keys: &[
            ("NoModifier", 0),
            ("ShiftModifier", 33554432),
            ("ControlModifier", 67108864),
            ("AltModifier", 134217728),
            ("MetaModifier", 268435456),
            ("KeypadModifier", 536870912),
            ("GroupSwitchModifier", 1073741824),
            ("KeyboardModifierMask", -33554432),
        ],
    },
    QtEnum {
        name: "Modifiers",
        alias: "Modifier",
        keys: &[
            ("META", 268435456),
            ("SHIFT", 33554432),
            ("CTRL", 67108864),
            ("ALT", 134217728),
            ("MODIFIER_MASK", -33554432),
        ],
    },
    QtEnum {
        name: "ArrowType",
        alias: "",
        keys: &[
            ("NoArrow", 0),
            ("UpArrow", 1),
            ("DownArrow", 2),
            ("LeftArrow", 3),
            ("RightArrow", 4),
        ],
    },
    QtEnum {
        name: "PenStyle",
        alias: "",
        keys: &[
            ("NoPen", 0),
            ("SolidLine", 1),
            ("DashLine", 2),
            ("DotLine", 3),
            ("DashDotLine", 4),
            ("DashDotDotLine", 5),
            ("CustomDashLine", 6),
        ],
    },
    QtEnum {
        name: "PenCapStyle",
        alias: "",
        keys: &[
            ("FlatCap", 0),
            ("SquareCap", 16),
            ("RoundCap", 32),
            ("MPenCapStyle", 48),
        ],
    },
    QtEnum {
        name: "PenJoinStyle",
        alias: "",
        keys: &[
            ("MiterJoin", 0),
            ("BevelJoin", 64),
            ("RoundJoin", 128),
            ("SvgMiterJoin", 256),
            ("MPenJoinStyle", 448),
        ],
    },
    QtEnum {
        name: "BrushStyle",
        alias: "",
        keys: &[
            ("NoBrush", 0),
            ("SolidPattern", 1),
            ("Dense1Pattern", 2),
            ("Dense2Pattern", 3),
            ("Dense3Pattern", 4),
            ("Dense4Pattern", 5),
            ("Dense5Pattern", 6),
            ("Dense6Pattern", 7),
            ("Dense7Pattern", 8),
            ("HorPattern", 9),
            ("VerPattern", 10),
            ("CrossPattern", 11),
            ("BDiagPattern", 12),
            ("FDiagPattern", 13),
            ("DiagCrossPattern", 14),
            ("LinearGradientPattern", 15),
            ("RadialGradientPattern", 16),
            ("ConicalGradientPattern", 17),
            ("TexturePattern", 24),
        ],
    },
    QtEnum {
        name: "SizeMode",
        alias: "",
        keys: &[("AbsoluteSize", 0), ("RelativeSize", 1)],
    },
    QtEnum {
        name: "CursorShape",
        alias: "",
        keys: &[
            ("ArrowCursor", 0),
            ("UpArrowCursor", 1),
            ("CrossCursor", 2),
            ("WaitCursor", 3),
            ("IBeamCursor", 4),
            ("SizeVerCursor", 5),
            ("SizeHorCursor", 6),
            ("SizeBDiagCursor", 7),
            ("SizeFDiagCursor", 8),
            ("SizeAllCursor", 9),
            ("BlankCursor", 10),
            ("SplitVCursor", 11),
            ("SplitHCursor", 12),
            ("PointingHandCursor", 13),
            ("ForbiddenCursor", 14),
            ("WhatsThisCursor", 15),
            ("BusyCursor", 16),
            ("OpenHandCursor", 17),
            ("ClosedHandCursor", 18),
            ("DragCopyCursor", 19),
            ("DragMoveCursor", 20),
            ("DragLinkCursor", 21),
            ("LastCursor", 21),
            ("BitmapCursor", 24),
            ("CustomCursor", 25),
        ],
    },
    QtEnum {
        name: "TextFormat",
        alias: "",
        keys: &[
            ("PlainText", 0),
            ("RichText", 1),
            ("AutoText", 2),
            ("MarkdownText", 3),
        ],
    },
    QtEnum {
        name: "AspectRatioMode",
        alias: "",
        keys: &[
            ("IgnoreAspectRatio", 0),
            ("KeepAspectRatio", 1),
            ("KeepAspectRatioByExpanding", 2),
        ],
    },
    QtEnum {
        name: "DockWidgetAreas",
        alias: "DockWidgetArea",
        keys: &[
            ("LeftDockWidgetArea", 1),
            ("RightDockWidgetArea", 2),
            ("TopDockWidgetArea", 4),
            ("BottomDockWidgetArea", 8),
            ("DockWidgetArea_Mask", 15),
            ("AllDockWidgetAreas", 15),
            ("NoDockWidgetArea", 0),
        ],
    },
    QtEnum {
        name: "ToolBarAreas",
        alias: "ToolBarArea",
        keys: &[
            ("LeftToolBarArea", 1),
            ("RightToolBarArea", 2),
            ("TopToolBarArea", 4),
            ("BottomToolBarArea", 8),
            ("ToolBarArea_Mask", 15),
            ("AllToolBarAreas", 15),
            ("NoToolBarArea", 0),
        ],
    },
    QtEnum {
        name: "DateFormat",
        alias: "",
        keys: &[
            ("TextDate", 0),
            ("ISODate", 1),
            ("RFC2822Date", 8),
            ("ISODateWithMs", 9),
        ],
    },
    QtEnum {
        name: "TimeSpec",
        alias: "",
        keys: &[
            ("LocalTime", 0),
            ("UTC", 1),
            ("OffsetFromUTC", 2),
            ("TimeZone", 3),
        ],
    },
    QtEnum {
        name: "DayOfWeek",
        alias: "",
        keys: &[
            ("Monday", 1),
            ("Tuesday", 2),
            ("Wednesday", 3),
            ("Thursday", 4),
            ("Friday", 5),
            ("Saturday", 6),
            ("Sunday", 7),
        ],
    },
    QtEnum {
        name: "ScrollBarPolicy",
        alias: "",
        keys: &[
            ("ScrollBarAsNeeded", 0),
            ("ScrollBarAlwaysOff", 1),
            ("ScrollBarAlwaysOn", 2),
        ],
    },
    QtEnum {
        name: "CaseSensitivity",
        alias: "",
        keys: &[("CaseInsensitive", 0), ("CaseSensitive", 1)],
    },
    QtEnum {
        name: "Corner",
        alias: "",
        keys: &[
            ("TopLeftCorner", 0),
            ("TopRightCorner", 1),
            ("BottomLeftCorner", 2),
            ("BottomRightCorner", 3),
        ],
    },
    QtEnum {
        name: "Edges",
        alias: "Edge",
        keys: &[
            ("TopEdge", 1),
            ("LeftEdge", 2),
            ("RightEdge", 4),
            ("BottomEdge", 8),
        ],
    },
    QtEnum {
        name: "ConnectionType",
        alias: "",
        keys: &[
            ("AutoConnection", 0),
            ("DirectConnection", 1),
            ("QueuedConnection", 2),
            ("BlockingQueuedConnection", 3),
            ("UniqueConnection", 128),
            ("SingleShotConnection", 256),
        ],
    },
    QtEnum {
        name: "ShortcutContext",
        alias: "",
        keys: &[
            ("WidgetShortcut", 0),
            ("WindowShortcut", 1),
            ("ApplicationShortcut", 2),
            ("WidgetWithChildrenShortcut", 3),
        ],
    },
    QtEnum {
        name: "FillRule",
        alias: "",
        keys: &[("OddEvenFill", 0), ("WindingFill", 1)],
    },
    QtEnum {
        name: "MaskMode",
        alias: "",
        keys: &[("MaskInColor", 0), ("MaskOutColor", 1)],
    },
    QtEnum {
        name: "ClipOperation",
        alias: "",
        keys: &[("NoClip", 0), ("ReplaceClip", 1), ("IntersectClip", 2)],
    },
    QtEnum {
        name: "ItemSelectionMode",
        alias: "",
        keys: &[
            ("ContainsItemShape", 0),
            ("IntersectsItemShape", 1),
            ("ContainsItemBoundingRect", 2),
            ("IntersectsItemBoundingRect", 3),
        ],
    },
    QtEnum {
        name: "ItemSelectionOperation",
        alias: "",
        keys: &[("ReplaceSelection", 0), ("AddToSelection", 1)],
    },
    QtEnum {
        name: "TransformationMode",
        alias: "",
        keys: &[("FastTransformation", 0), ("SmoothTransformation", 1)],
    },
    QtEnum {
        name: "Axis",
        alias: "",
        keys: &[("XAxis", 0), ("YAxis", 1), ("ZAxis", 2)],
    },
    QtEnum {
        name: "FocusReason",
        alias: "",
        keys: &[
            ("MouseFocusReason", 0),
            ("TabFocusReason", 1),
            ("BacktabFocusReason", 2),
            ("ActiveWindowFocusReason", 3),
            ("PopupFocusReason", 4),
            ("ShortcutFocusReason", 5),
            ("MenuBarFocusReason", 6),
            ("OtherFocusReason", 7),
            ("NoFocusReason", 8),
        ],
    },
    QtEnum {
        name: "ContextMenuPolicy",
        alias: "",
        keys: &[
            ("NoContextMenu", 0),
            ("DefaultContextMenu", 1),
            ("ActionsContextMenu", 2),
            ("CustomContextMenu", 3),
            ("PreventContextMenu", 4),
        ],
    },
    QtEnum {
        name: "ContextMenuTrigger",
        alias: "",
        keys: &[("Press", 0), ("Release", 1)],
    },
    QtEnum {
        name: "InputMethodQueries",
        alias: "InputMethodQuery",
        keys: &[
            ("ImEnabled", 1),
            ("ImCursorRectangle", 2),
            ("ImFont", 4),
            ("ImCursorPosition", 8),
            ("ImSurroundingText", 16),
            ("ImCurrentSelection", 32),
            ("ImMaximumTextLength", 64),
            ("ImAnchorPosition", 128),
            ("ImHints", 256),
            ("ImPreferredLanguage", 512),
            ("ImAbsolutePosition", 1024),
            ("ImTextBeforeCursor", 2048),
            ("ImTextAfterCursor", 4096),
            ("ImEnterKeyType", 8192),
            ("ImAnchorRectangle", 16384),
            ("ImInputItemClipRectangle", 32768),
            ("ImReadOnly", 65536),
            ("ImPlatformData", -2147483648),
            ("ImQueryInput", 16570),
            ("ImQueryAll", -1),
        ],
    },
    QtEnum {
        name: "InputMethodHints",
        alias: "InputMethodHint",
        keys: &[
            ("ImhNone", 0),
            ("ImhHiddenText", 1),
            ("ImhSensitiveData", 2),
            ("ImhNoAutoUppercase", 4),
            ("ImhPreferNumbers", 8),
            ("ImhPreferUppercase", 16),
            ("ImhPreferLowercase", 32),
            ("ImhNoPredictiveText", 64),
            ("ImhDate", 128),
            ("ImhTime", 256),
            ("ImhPreferLatin", 512),
            ("ImhMultiLine", 1024),
            ("ImhNoEditMenu", 2048),
            ("ImhNoTextHandles", 4096),
            ("ImhDigitsOnly", 65536),
            ("ImhFormattedNumbersOnly", 131072),
            ("ImhUppercaseOnly", 262144),
            ("ImhLowercaseOnly", 524288),
            ("ImhDialableCharactersOnly", 1048576),
            ("ImhEmailCharactersOnly", 2097152),
            ("ImhUrlCharactersOnly", 4194304),
            ("ImhLatinOnly", 8388608),
            ("ImhExclusiveInputMask", -65536),
        ],
    },
    QtEnum {
        name: "EnterKeyType",
        alias: "",
        keys: &[
            ("EnterKeyDefault", 0),
            ("EnterKeyReturn", 1),
            ("EnterKeyDone", 2),
            ("EnterKeyGo", 3),
            ("EnterKeySend", 4),
            ("EnterKeySearch", 5),
            ("EnterKeyNext", 6),
            ("EnterKeyPrevious", 7),
        ],
    },
    QtEnum {
        name: "ToolButtonStyle",
        alias: "",
        keys: &[
            ("ToolButtonIconOnly", 0),
            ("ToolButtonTextOnly", 1),
            ("ToolButtonTextBesideIcon", 2),
            ("ToolButtonTextUnderIcon", 3),
            ("ToolButtonFollowStyle", 4),
        ],
    },
    QtEnum {
        name: "LayoutDirection",
        alias: "",
        keys: &[
            ("LeftToRight", 0),
            ("RightToLeft", 1),
            ("LayoutDirectionAuto", 2),
        ],
    },
    QtEnum {
        name: "DropActions",
        alias: "DropAction",
        keys: &[
            ("CopyAction", 1),
            ("MoveAction", 2),
            ("LinkAction", 4),
            ("ActionMask", 255),
            ("TargetMoveAction", 32770),
            ("IgnoreAction", 0),
        ],
    },
    QtEnum {
        name: "CheckState",
        alias: "",
        keys: &[("Unchecked", 0), ("PartiallyChecked", 1), ("Checked", 2)],
    },
    QtEnum {
        name: "ItemDataRole",
        alias: "",
        keys: &[
            ("DisplayRole", 0),
            ("DecorationRole", 1),
            ("EditRole", 2),
            ("ToolTipRole", 3),
            ("StatusTipRole", 4),
            ("WhatsThisRole", 5),
            ("FontRole", 6),
            ("TextAlignmentRole", 7),
            ("BackgroundRole", 8),
            ("ForegroundRole", 9),
            ("CheckStateRole", 10),
            ("AccessibleTextRole", 11),
            ("AccessibleDescriptionRole", 12),
            ("SizeHintRole", 13),
            ("InitialSortOrderRole", 14),
            ("DisplayPropertyRole", 27),
            ("DecorationPropertyRole", 28),
            ("ToolTipPropertyRole", 29),
            ("StatusTipPropertyRole", 30),
            ("WhatsThisPropertyRole", 31),
            ("RangeModelDataRole", 40),
            ("RangeModelAdapterRole", 41),
            ("UserRole", 256),
            ("StandardItemFlagsRole", 255),
            ("FileInfoRole", 252),
            ("RemoteObjectsCacheRole", 255),
        ],
    },
    QtEnum {
        name: "ItemFlags",
        alias: "ItemFlag",
        keys: &[
            ("NoItemFlags", 0),
            ("ItemIsSelectable", 1),
            ("ItemIsEditable", 2),
            ("ItemIsDragEnabled", 4),
            ("ItemIsDropEnabled", 8),
            ("ItemIsUserCheckable", 16),
            ("ItemIsEnabled", 32),
            ("ItemIsAutoTristate", 64),
            ("ItemNeverHasChildren", 128),
            ("ItemIsUserTristate", 256),
        ],
    },
    QtEnum {
        name: "MatchFlags",
        alias: "MatchFlag",
        keys: &[
            ("MatchExactly", 0),
            ("MatchContains", 1),
            ("MatchStartsWith", 2),
            ("MatchEndsWith", 3),
            ("MatchRegularExpression", 4),
            ("MatchWildcard", 5),
            ("MatchFixedString", 8),
            ("MatchTypeMask", 15),
            ("MatchCaseSensitive", 16),
            ("MatchWrap", 32),
            ("MatchRecursive", 64),
        ],
    },
    QtEnum {
        name: "WindowModality",
        alias: "",
        keys: &[("NonModal", 0), ("WindowModal", 1), ("ApplicationModal", 2)],
    },
    QtEnum {
        name: "TextInteractionFlags",
        alias: "TextInteractionFlag",
        keys: &[
            ("NoTextInteraction", 0),
            ("TextSelectableByMouse", 1),
            ("TextSelectableByKeyboard", 2),
            ("LinksAccessibleByMouse", 4),
            ("LinksAccessibleByKeyboard", 8),
            ("TextEditable", 16),
            ("TextEditorInteraction", 19),
            ("TextBrowserInteraction", 13),
        ],
    },
    QtEnum {
        name: "SizeHint",
        alias: "",
        keys: &[
            ("MinimumSize", 0),
            ("PreferredSize", 1),
            ("MaximumSize", 2),
            ("MinimumDescent", 3),
            ("NSizeHints", 4),
        ],
    },
    QtEnum {
        name: "TouchPointStates",
        alias: "TouchPointState",
        keys: &[
            ("TouchPointUnknownState", 0),
            ("TouchPointPressed", 1),
            ("TouchPointMoved", 2),
            ("TouchPointStationary", 4),
            ("TouchPointReleased", 8),
        ],
    },
    QtEnum {
        name: "GestureState",
        alias: "",
        keys: &[
            ("NoGesture", 0),
            ("GestureStarted", 1),
            ("GestureUpdated", 2),
            ("GestureFinished", 3),
            ("GestureCanceled", 4),
        ],
    },
    QtEnum {
        name: "GestureType",
        alias: "",
        keys: &[
            ("TapGesture", 1),
            ("TapAndHoldGesture", 2),
            ("PanGesture", 3),
            ("PinchGesture", 4),
            ("SwipeGesture", 5),
            ("CustomGesture", 256),
            ("LastGestureType", -1),
        ],
    },
    QtEnum {
        name: "NativeGestureType",
        alias: "",
        keys: &[
            ("BeginNativeGesture", 0),
            ("EndNativeGesture", 1),
            ("PanNativeGesture", 2),
            ("ZoomNativeGesture", 3),
            ("SmartZoomNativeGesture", 4),
            ("RotateNativeGesture", 5),
            ("SwipeNativeGesture", 6),
        ],
    },
    QtEnum {
        name: "CursorMoveStyle",
        alias: "",
        keys: &[("LogicalMoveStyle", 0), ("VisualMoveStyle", 1)],
    },
    QtEnum {
        name: "TimerType",
        alias: "",
        keys: &[
            ("PreciseTimer", 0),
            ("CoarseTimer", 1),
            ("VeryCoarseTimer", 2),
        ],
    },
    QtEnum {
        name: "TimerId",
        alias: "",
        keys: &[("Invalid", 0)],
    },
    QtEnum {
        name: "ScrollPhase",
        alias: "",
        keys: &[
            ("NoScrollPhase", 0),
            ("ScrollBegin", 1),
            ("ScrollUpdate", 2),
            ("ScrollEnd", 3),
            ("ScrollMomentum", 4),
        ],
    },
    QtEnum {
        name: "MouseEventSource",
        alias: "",
        keys: &[
            ("MouseEventNotSynthesized", 0),
            ("MouseEventSynthesizedBySystem", 1),
            ("MouseEventSynthesizedByQt", 2),
            ("MouseEventSynthesizedByApplication", 3),
        ],
    },
    QtEnum {
        name: "MouseEventFlags",
        alias: "MouseEventFlag",
        keys: &[
            ("NoMouseEventFlag", 0),
            ("MouseEventCreatedDoubleClick", 1),
            ("MouseEventFlagMask", 255),
        ],
    },
    QtEnum {
        name: "ChecksumType",
        alias: "",
        keys: &[("ChecksumIso3309", 0), ("ChecksumItuV41", 1)],
    },
    QtEnum {
        name: "HighDpiScaleFactorRoundingPolicy",
        alias: "",
        keys: &[
            ("Unset", 0),
            ("Round", 1),
            ("Ceil", 2),
            ("Floor", 3),
            ("RoundPreferFloor", 4),
            ("PassThrough", 5),
        ],
    },
    QtEnum {
        name: "PermissionStatus",
        alias: "",
        keys: &[("Undetermined", 0), ("Granted", 1), ("Denied", 2)],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_enum_is_found_by_either_name() {
        let flags = qt_enum("Alignment").expect("the flags");
        assert!(std::ptr::eq(
            flags,
            qt_enum("AlignmentFlag").expect("one key")
        ));
        assert!(flags.keys.contains(&("AlignVCenter", 0x80)));
        assert!(qt_enum("LayoutDirection").is_some());
        assert!(qt_enum("").is_none());
        assert!(qt_enum("Nowhere").is_none());
    }

    #[test]
    fn a_key_has_one_number_whichever_enum_lists_it() {
        let mut seen = std::collections::HashMap::new();
        for (k, v) in QT_ENUMS.iter().flat_map(|e| e.keys.iter()) {
            assert_eq!(*seen.entry(*k).or_insert(*v), *v, "{k}");
        }
    }

    #[test]
    fn the_older_tables_agree() {
        for (k, v) in crate::js::QT_ENUM_KEYS
            .iter()
            .chain(crate::qt_keys::QT_KEY_ENUMS)
        {
            if let Some(here) = key(k) {
                assert_eq!(here, *v, "{k}");
            }
        }
    }
}
