//! The Ljós backend. The Engine does not exist yet, so every operation reports
//! "not implemented yet"; `@qt-parity` scenarios therefore fail here (pending)
//! while they already pass on Qt. Replace the bodies as the Engine lands.

use serde_json::Value;

use super::{A11yNode, Backend, BackendError, LogEntry, QmlFile, Result};

const WHY: &str = "the Ljós engine is not built yet";

#[derive(Debug, Default)]
pub struct LjosBackend;

fn pending<T>() -> Result<T> {
    Err(BackendError::NotImplemented(WHY))
}

impl Backend for LjosBackend {
    fn name(&self) -> &'static str {
        "ljos"
    }
    fn load(&mut self, _: &[QmlFile], _: &str) -> Result<()> {
        pending()
    }
    fn set(&mut self, _: &str, _: &str, _: &Value) -> Result<()> {
        pending()
    }
    fn bind(&mut self, _: &str, _: &str, _: &str) -> Result<()> {
        pending()
    }
    fn get(&mut self, _: &str, _: &str) -> Result<Value> {
        pending()
    }
    fn call(&mut self, _: &str, _: &str, _: &[Value]) -> Result<Value> {
        pending()
    }
    fn exists(&mut self, _: &str) -> Result<bool> {
        pending()
    }
    fn advance(&mut self, _: u64) -> Result<()> {
        pending()
    }
    fn click(&mut self, _: &str, _: &str) -> Result<()> {
        pending()
    }
    fn key(&mut self, _: &str) -> Result<()> {
        pending()
    }
    fn touch(&mut self, _: f64, _: f64) -> Result<()> {
        pending()
    }
    fn a11y_tree(&mut self) -> Result<A11yNode> {
        pending()
    }
    fn log(&mut self) -> Result<Vec<LogEntry>> {
        pending()
    }
    fn grab(&mut self) -> Result<Vec<u8>> {
        pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operation_is_pending() {
        let mut b = LjosBackend;
        assert!(matches!(
            b.load(&[], "x.qml"),
            Err(BackendError::NotImplemented(_))
        ));
        assert!(matches!(
            b.get("a", "b"),
            Err(BackendError::NotImplemented(_))
        ));
        assert!(matches!(b.advance(1), Err(BackendError::NotImplemented(_))));
    }
}
