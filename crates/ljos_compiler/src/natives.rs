//! What the compat pass is told of a program's own types: what the
//! checker's [`TypeOracle`] has of them, so a file is lowered against the
//! types it is checked against. Ljós's own modules (`Ljos.Design`) are told
//! as such too: their types are in Ljós's API already, so a file that
//! imports one beside Qt's keeps that import as it is written and lowers
//! the rest, as it does a module of the program's.

use std::cell::RefCell;

use qml_compat::{Holds, Native, is_ljos_api};
use qml_front::{QtTypeOracle, TypeOracle};

/// The native types `oracle` has beside the table of types, as
/// [`qml_compat::Natives`].
pub struct OracleNatives<'o, O: TypeOracle> {
    oracle: &'o O,
    /// The types handed out, by their [`Native`] number.
    types: RefCell<Vec<O::Type>>,
}

impl<O: TypeOracle> std::fmt::Debug for OracleNatives<'_, O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OracleNatives")
            .field("types", &self.types.borrow().len())
            .finish_non_exhaustive()
    }
}

impl<'o, O: TypeOracle> OracleNatives<'o, O> {
    pub fn new(oracle: &'o O) -> Self {
        OracleNatives {
            oracle,
            types: RefCell::default(),
        }
    }

    fn number(&self, ty: O::Type) -> Native {
        let mut types = self.types.borrow_mut();
        let at = types.iter().position(|t| *t == ty).unwrap_or_else(|| {
            types.push(ty);
            types.len() - 1
        });
        Native(u32::try_from(at).expect("fewer native types than u32 numbers"))
    }

    fn ty(&self, native: Native) -> O::Type {
        self.types.borrow()[native.0 as usize]
    }

    /// Whether `ty` is a type of the table in Qt's API, the toolkit's.
    fn is_toolkit(&self, ty: O::Type) -> bool {
        self.oracle.module(ty).is_some_and(|module| {
            !is_ljos_api(&module)
                && QtTypeOracle
                    .module_type(&module, self.oracle.name(ty))
                    .is_some()
        })
    }
}

impl<O: TypeOracle> qml_compat::Natives for OracleNatives<'_, O> {
    fn has_module(&self, uri: &str) -> bool {
        // A module of the table's in Qt's API is the toolkit's, whatever
        // the program registers in it.
        self.oracle.has_module(uri) && (is_ljos_api(uri) || !QtTypeOracle.has_module(uri))
    }

    fn module_type(&self, uri: &str, name: &str) -> Option<Native> {
        if !self.has_module(uri) {
            return None;
        }
        let ty = self.oracle.module_type(uri, name)?;
        Some(self.number(ty))
    }

    fn name(&self, ty: Native) -> String {
        self.oracle.name(self.ty(ty)).to_owned()
    }

    fn is_singleton(&self, ty: Native) -> bool {
        self.oracle.is_singleton(self.ty(ty))
    }

    fn has_member(&self, ty: Native, name: &str) -> bool {
        let ty = self.ty(ty);
        let oracle = self.oracle;
        oracle.property(ty, name).is_some()
            || oracle.signal(ty, name).is_some()
            || oracle.has_method(ty, name)
            || oracle.has_enum(ty, name)
    }

    fn property(&self, ty: Native, name: &str) -> Option<Holds> {
        let property = self.oracle.property(self.ty(ty), name)?;
        Some(match property.value {
            Some(held) if self.is_toolkit(held) => Holds::Toolkit {
                module: self.oracle.module(held)?,
                name: self.oracle.name(held).to_owned(),
            },
            Some(held) => Holds::Native(self.number(held)),
            None => Holds::Value,
        })
    }

    fn toolkit_base(&self, ty: Native) -> Option<(String, String)> {
        let ty = self.ty(ty);
        let oracle = self.oracle;
        // A type of Ljós's own table is written as built on the toolkit's
        // through its prototypes (`Slot` on `QtQuick.Item`), as
        // qml_compat's own description of it has it too.
        let base = oracle.toolkit_base(ty).or_else(|| {
            oracle.module(ty).filter(|module| is_ljos_api(module))?;
            std::iter::successors(oracle.prototype(ty), |t| oracle.prototype(*t)).find(|t| {
                oracle
                    .module(*t)
                    .is_some_and(|module| !is_ljos_api(&module))
            })
        })?;
        Some((self.oracle.module(base)?, self.oracle.name(base).to_owned()))
    }

    fn has_ljos_names(&self, ty: Native) -> bool {
        self.oracle
            .module(self.ty(ty))
            .is_some_and(|module| is_ljos_api(&module))
    }
}

#[cfg(test)]
mod tests {
    use qml_compat::Natives;

    use super::*;

    // The CLI lowers through these natives, not qml_compat's own
    // description: a type of Ljós's own must be told the same way there.
    #[test]
    fn a_type_of_ljos_s_own_has_ljos_s_names_on_the_toolkit_type_it_is_built_on() {
        let natives = OracleNatives::new(&QtTypeOracle);
        let slot = natives.module_type("Ljos.Blocks", "Slot").expect("Slot");
        assert!(natives.has_ljos_names(slot));
        assert_eq!(
            natives.toolkit_base(slot),
            Some(("QtQuick".to_owned(), "Item".to_owned()))
        );
        assert!(natives.module_type("QtQuick", "Item").is_none());
    }
}
