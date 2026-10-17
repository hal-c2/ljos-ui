use std::sync::Arc;

use rquickjs::{
    Context, Module, Runtime,
    module::{WriteOptions, WriteOptionsEndianness},
};

use crate::{
    JsFault,
    host::{Sources, classify_error},
};

/// Options for [`Bytecode::compile`].
#[derive(Debug, Clone, Copy)]
pub struct BytecodeOptions {
    /// Drop the source text (`Function.prototype.toString` then prints
    /// `[native code]`). Saves about half the size.
    pub strip_source: bool,
    /// Drop line tables. Exceptions then carry no line numbers, so leave this
    /// off unless size matters more than diagnostics.
    pub strip_debug: bool,
}

impl Default for BytecodeOptions {
    fn default() -> Self {
        Self {
            strip_source: true,
            strip_debug: false,
        }
    }
}

/// Compiled module bytecode.
///
/// The format is private to the linked QuickJS version (rquickjs 0.14 /
/// quickjs-ng 0.16.2 is `BC_VERSION` 28) and is not stable across upgrades:
/// treat it as a cache and always keep the source (research 03 risk 10).
/// QuickJS does not validate bytecode beyond a version tag, so it is only
/// ever produced by [`Bytecode::compile`] or declared trusted by the unsafe
/// [`Bytecode::from_trusted`].
///
/// It is `Send`, so it can be compiled on the compute pool and loaded on the
/// UI thread.
#[derive(Debug, Clone)]
pub struct Bytecode(Arc<[u8]>);

impl Bytecode {
    /// Compiles `source` as an ES module named `name` in a throwaway runtime,
    /// so this works on any thread.
    pub fn compile(name: &str, source: &str, opts: BytecodeOptions) -> Result<Self, JsFault> {
        let rt = Runtime::new()?;
        let ctx = Context::full(&rt)?;
        let bytes = ctx.with(|ctx| {
            let module = Module::declare(ctx.clone(), name, source).and_then(|m| {
                m.write(WriteOptions {
                    endianness: WriteOptionsEndianness::Native,
                    strip_source: opts.strip_source,
                    strip_debug: opts.strip_debug,
                    ..WriteOptions::default()
                })
            });
            module.map_err(|e| {
                classify_error(
                    &ctx,
                    None,
                    e,
                    name,
                    false,
                    std::time::Duration::ZERO,
                    Sources::default(),
                )
            })
        })?;
        Ok(Self(bytes.into()))
    }

    /// # Safety
    /// `bytes` must have been produced by [`Bytecode::as_bytes`] of the same
    /// QuickJS version and not modified since (for example read back from our
    /// own cache directory after checking a content hash).
    pub unsafe fn from_trusted(bytes: Vec<u8>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub(crate) fn shared(&self) -> Arc<[u8]> {
        self.0.clone()
    }
}
