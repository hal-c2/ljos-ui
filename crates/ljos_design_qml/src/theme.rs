//! A theme ([`ljos_design::theme`]) put on the singletons of the module.
//!
//! Each role the theme sets is an override on the singleton's property,
//! so what binds to `Colors.accent` follows without being made again, and
//! taking the theme away gives back the module's own value. A colour the
//! theme gives only some variants of is a binding: it reads the colour
//! scheme and contrast the module reads, and for a variant the theme does
//! not give it reads the module's own colour from `Colors.roles`.

use std::cell::RefCell;

use ljos_design::palette::ROLES;
use ljos_design::theme::{Entry, Setting, Theme, ThemeError};
use ljos_engine::{
    Color, Engine, ObjectId, OverrideContent, OverrideId, PropIndex, RestoreMode, Value,
};

use crate::URI;

/// The overrides of the theme the engine shows, and the seed hue the
/// application had beneath a theme that set its own.
#[derive(Default)]
struct Applied {
    overrides: RefCell<Vec<(ObjectId, PropIndex, OverrideId)>>,
    own_seed: RefCell<Option<f64>>,
}

/// Puts `theme` on the module in place of the one before; `None` takes
/// the theme away. The theme's colours are held to the contrast the
/// palette promises first ([`Theme::check_contrast`]), against the seed
/// hue the application runs with.
///
/// # Errors
/// The theme fails the contrast check, or names a role this build of the
/// module does not have. Then the theme before stays as it was.
pub fn apply(engine: &Engine, theme: Option<&Theme>) -> Result<(), Vec<ThemeError>> {
    let applied = engine.extension::<Applied>();
    let seed = own_seed(engine, &applied);
    if let Some(theme) = theme {
        theme.check_contrast(seed)?;
    }
    let mut pushes = Vec::new();
    if let Some(theme) = theme {
        let mut errors = Vec::new();
        for e in &theme.entries {
            match target(engine, e) {
                Ok(t) => pushes.push((t, e)),
                Err(message) => errors.push(ThemeError {
                    file: theme.file.clone(),
                    line: e.at.line,
                    column: e.at.column,
                    message,
                }),
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
    }
    let failed = engine.batch(|| {
        for (obj, prop, id) in applied.overrides.take() {
            // The singleton outlives every theme; an override that is gone
            // went with it, and there is nothing to give back.
            let _ = engine.remove_override(obj, prop, id, RestoreMode::BindingOrValue);
        }
        *applied.own_seed.borrow_mut() = None;
        let mut errors = Vec::new();
        let mut done = Vec::new();
        for ((obj, prop), e) in pushes {
            if e.name == "seedHue" {
                *applied.own_seed.borrow_mut() = seed;
            }
            match engine.push_override(obj, prop, content(engine, e)) {
                Ok(id) => done.push((obj, prop, id)),
                Err(err) => errors.push((e, err.to_string())),
            }
        }
        *applied.overrides.borrow_mut() = done;
        errors
    });
    // Only a value this build of the module cannot hold gets here, as the
    // reader knows every role and kind: the rest of the theme stands.
    if failed.is_empty() {
        Ok(())
    } else {
        let file = theme.map(|t| t.file.clone()).unwrap_or_default();
        Err(failed
            .into_iter()
            .map(|(e, message)| ThemeError {
                file: file.clone(),
                line: e.at.line,
                column: e.at.column,
                message: format!("{}.{}: {message}", e.singleton, e.name),
            })
            .collect())
    }
}

/// The seed hue of the application, beneath the theme's when it set one.
fn own_seed(engine: &Engine, applied: &Applied) -> Option<f64> {
    if let Some(seed) = *applied.own_seed.borrow() {
        return Some(seed);
    }
    let colors = engine.singleton_by_name(URI, "Colors")?;
    let prop = engine.find_property(colors, "seedHue")?;
    match engine.untrack(|| engine.read(colors, prop)) {
        Value::Real(h) => Some(h),
        Value::Int(h) => Some(f64::from(h)),
        _ => None,
    }
}

fn target(engine: &Engine, e: &Entry) -> Result<(ObjectId, PropIndex), String> {
    let obj = engine
        .singleton_by_name(URI, e.singleton)
        .ok_or_else(|| format!("{URI} has no singleton {} in this engine", e.singleton))?;
    let prop = engine
        .find_property(obj, &e.name)
        .ok_or_else(|| format!("{}.{} is not in this build of {URI}", e.singleton, e.name))?;
    Ok((obj, prop))
}

fn content(engine: &Engine, e: &Entry) -> OverrideContent {
    match &e.value {
        Setting::Int(n) => OverrideContent::Value(Value::Int(*n)),
        Setting::Real(n) => OverrideContent::Value(Value::Real(*n)),
        Setting::Curve(points) => OverrideContent::Value(Value::List(
            points
                .iter()
                .map(|&p| Value::Real(p))
                .collect::<Vec<_>>()
                .into(),
        )),
        Setting::Color(variants) => {
            let pick = |c: &Option<String>| c.as_deref().and_then(Color::parse);
            let light = pick(&variants.light);
            let dark = pick(&variants.dark);
            let light_high = pick(&variants.light_high);
            let dark_high = pick(&variants.dark_high);
            let index = ROLES
                .iter()
                .position(|r| ljos_design::module::qml_role(r) == e.name);
            let colors = engine.singleton_by_name(URI, "Colors");
            let prop = |name| colors.and_then(|c| engine.find_property(c, name));
            let (is_dark, is_high, roles) = (prop("dark"), prop("highContrast"), prop("roles"));
            OverrideContent::Binding(Box::new(move |engine| {
                let read = |p: Option<PropIndex>| match (colors, p) {
                    (Some(c), Some(p)) => engine.read(c, p),
                    _ => Value::Undefined,
                };
                let ours = match (read(is_dark).to_boolean(), read(is_high).to_boolean()) {
                    (false, false) => light,
                    (true, false) => dark,
                    (false, true) => light_high,
                    (true, true) => dark_high,
                };
                if let Some(c) = ours {
                    return Value::Color(c);
                }
                let own = read(roles);
                index
                    .and_then(|i| own.as_list()?.get(i).cloned())
                    .unwrap_or_default()
            }))
        }
    }
}
