//! Every property Qt can take back says so.
//!
//! `width = undefined` works because `width` declares a reset; a property
//! that forgot to refuses `undefined` as a value of the wrong type, and
//! nothing fails to compile. So this goes over every registered type: what
//! Qt's type description gives a reset has one here, or is in the list of
//! what is still to do, and nothing here has a reset Qt does not have.

use ljos_engine::{Engine, MetaRef};
use qml_front::{QtType, QtTypeOracle, TypeOracle};

fn engine() -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    e
}

/// Every registered type and what it attaches, with the name to report.
fn metas(e: &Engine) -> Vec<(String, String, String, MetaRef)> {
    let mut out = Vec::new();
    for module in &e.type_dump().modules {
        for ty in &module.types {
            let meta = e
                .lookup_type(&module.uri, &ty.name)
                .expect("a dumped type is registered");
            if let Some(attached) = meta.attached {
                let name = format!("{} (attached)", ty.name);
                out.push((module.uri.clone(), ty.name.clone(), name, attached()));
            }
            out.push((module.uri.clone(), ty.name.clone(), ty.name.clone(), meta));
        }
    }
    out
}

/// Whether Qt takes the property back, if it has it. `anchors.leftMargin`
/// is the member of the group's type.
fn qt_reset(ty: QtType, name: &str) -> Option<bool> {
    let mut ty = ty.0;
    let mut parts = name.split('.').peekable();
    loop {
        let property = ty.property(parts.next()?)?;
        if parts.peek().is_none() {
            return Some(property.has_reset);
        }
        ty = property.value_type()?;
    }
}

/// Properties Qt takes back and Ljós does not yet: `Type.property`, or
/// `*.property` for every type that has it; `group.*` is every member of a
/// group. Assigning `undefined` to one is refused. Take a line out when
/// the property gets its reset.
const NOT_YET: &[&str] = &[
    // What a control inherits when nobody gave it: taking one back means
    // resolving it against the parent again, which a default cannot say.
    "*.font",
    "*.locale",
    "*.palette",
    "*.palette.*",
    // Qt's controls take these back to what the content gives.
    "*.baselineOffset",
    "*.contentWidth",
    "*.contentHeight",
    // What a button shows follows its action again.
    "*.text",
    "*.icon.*",
    "*.down",
    "Action.enabled",
    "LayoutMirroring (attached).enabled",
    "ComboBox.displayText",
    "ComboBox.editText",
    "DialogButtonBox.alignment",
    "DialogButtonBox.buttonLayout",
    "Menu.cascade",
    "Page.title",
    "ScrollBar.interactive",
    // Back to the address of the document, which a text control does not
    // know here.
    "*.baseUrl",
];

/// Properties taken back here that Qt refuses to: each has the reset of the
/// type it is built on here, which in Qt is another type's property.
const BEYOND_QT: &[&str] = &[
    // A window is an item here; Qt's has a width of its own.
    "ApplicationWindow.height",
    "ApplicationWindow.width",
    "Window.height",
    "Window.width",
    // Shares the picture's property; Qt's can only be read.
    "BorderImage.sourceSize",
];

fn not_yet(ty: &str, property: &str) -> bool {
    NOT_YET.iter().any(|entry| {
        // The type may have a dot of its own only in "(attached)" names,
        // which have none: the first dot ends it.
        let Some((t, p)) = entry.split_once('.') else {
            return false;
        };
        let named = match p.strip_suffix(".*") {
            Some(group) => property
                .strip_prefix(group)
                .is_some_and(|rest| rest.starts_with('.')),
            None => p == property,
        };
        (t == "*" || t == ty) && named
    })
}

#[test]
fn what_qt_takes_back_is_taken_back_here() {
    // Without Qt's description every lookup finds nothing.
    if QtTypeOracle.module_type("QtQuick", "Item").is_none() {
        eprintln!("skipped: no Qt type description on this machine");
        return;
    }
    let known = QtTypeOracle
        .module_type("QtQuick", "Item")
        .and_then(|item| qt_reset(item, "width"))
        == Some(true);
    if !known {
        eprintln!("skipped: the type description predates resets; run `mise run types`");
        return;
    }
    let e = engine();
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    let mut done = Vec::new();
    let mut seen = 0;
    for (module, ty, name, meta) in metas(&e) {
        let Some(qt) = QtTypeOracle.module_type(&module, &ty) else {
            continue;
        };
        let qt = if name == ty {
            qt
        } else {
            match qt.0.attached() {
                Some(attached) => QtType(attached),
                None => continue,
            }
        };
        for property in &meta.properties {
            let Some(in_qt) = qt_reset(qt, &property.name) else {
                continue;
            };
            let here = property.reset.is_some();
            let listed = not_yet(&name, &property.name);
            match (in_qt, here) {
                (true, true) => {
                    seen += 1;
                    if listed {
                        done.push(format!("{name}.{}", property.name));
                    }
                }
                (true, false) if !listed => missing.push(format!("{name}.{}", property.name)),
                (false, true) => {
                    let which = format!("{name}.{}", property.name);
                    if !BEYOND_QT.contains(&which.as_str()) {
                        extra.push(which);
                    }
                }
                _ => {}
            }
        }
    }
    for list in [&mut missing, &mut extra, &mut done] {
        list.sort();
        list.dedup();
    }
    assert!(
        missing.is_empty(),
        "Qt takes these back, here they have no reset:\n{}",
        missing.join("\n")
    );
    assert!(
        extra.is_empty(),
        "these have a reset here and none in Qt:\n{}",
        extra.join("\n")
    );
    assert!(
        done.is_empty(),
        "these have a reset now; take them out of NOT_YET:\n{}",
        done.join("\n")
    );
    // A table that stopped matching anything would pass the same way.
    assert!(seen > 100, "{seen} properties with a reset were compared");
}
