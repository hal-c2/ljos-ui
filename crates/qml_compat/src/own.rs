//! Ljós's own modules (`Ljos.Blocks`, `Ljos.Design`), as the program's own
//! types where the caller describes none: their types and members have
//! Ljós's names already, so a file that imports one beside Qt's keeps that
//! import as it is written and lowers the rest. The checker's description
//! of a program's types has them too, and then that is what is asked.

use std::sync::Mutex;

use qt_types::Type;

use crate::{Holds, Native, Natives, is_ljos_api};

/// Ljós's own modules, as its table describes them.
pub(crate) struct Own;

/// The types handed out, by their [`Native`] number. Ljós's table has a
/// few hundred, so the list stays small whatever is lowered.
static TYPES: Mutex<Vec<&'static Type>> = Mutex::new(Vec::new());

fn number(ty: &'static Type) -> Native {
    let mut types = TYPES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let at = types
        .iter()
        .position(|t| std::ptr::eq(*t, ty))
        .unwrap_or_else(|| {
            types.push(ty);
            types.len() - 1
        });
    Native(u32::try_from(at).expect("fewer of Ljós's types than u32 numbers"))
}

fn ty(native: Native) -> &'static Type {
    TYPES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)[native.0 as usize]
}

impl Natives for Own {
    fn has_module(&self, uri: &str) -> bool {
        is_ljos_api(uri) && qt_types::module(uri).is_some_and(|module| module.is_own)
    }

    fn module_type(&self, uri: &str, name: &str) -> Option<Native> {
        if !self.has_module(uri) {
            return None;
        }
        qt_types::module(uri)?.type_named(name).map(number)
    }

    fn name(&self, ty: Native) -> String {
        self::ty(ty).name.to_owned()
    }

    fn is_singleton(&self, ty: Native) -> bool {
        self::ty(ty).is_singleton
    }

    fn has_member(&self, ty: Native, name: &str) -> bool {
        let ty = self::ty(ty);
        ty.property(name).is_some()
            || ty.signal(name).is_some()
            || ty.has_method(name)
            || ty.enums.iter().any(|e| e.name == name)
    }

    fn property(&self, ty: Native, name: &str) -> Option<Holds> {
        let property = self::ty(ty).property(name)?;
        Some(match property.value_type() {
            Some(held) if is_ljos_api(held.module) => Holds::Native(number(held)),
            Some(held) if !held.is_value => Holds::Toolkit {
                module: held.module.to_owned(),
                name: held.name.to_owned(),
            },
            _ => Holds::Value,
        })
    }

    // The table writes a type of Ljós's as built on the toolkit's
    // (`Slot` on `QtQuick.Item`): what it sets of that type's is checked
    // and renamed, and a name neither has is not passed over.
    fn toolkit_base(&self, ty: Native) -> Option<(String, String)> {
        let base = std::iter::successors(self::ty(ty).prototype(), |t| t.prototype())
            .find(|t| !is_ljos_api(t.module))?;
        Some((base.module.to_owned(), base.name.to_owned()))
    }

    fn has_ljos_names(&self, _ty: Native) -> bool {
        true
    }
}
