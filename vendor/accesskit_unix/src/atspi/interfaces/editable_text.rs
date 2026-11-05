// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

use accesskit_atspi_common::PlatformNode;
use zbus::{fdo, interface};

use crate::util::settle;

fn unsupported() -> fdo::Error {
    fdo::Error::NotSupported("editing operation is not supported".into())
}

pub(crate) struct EditableTextInterface(PlatformNode);

impl EditableTextInterface {
    pub fn new(node: PlatformNode) -> Self {
        Self(node)
    }

    fn map_error(&self) -> impl '_ + FnOnce(accesskit_atspi_common::Error) -> fdo::Error {
        |error| crate::util::map_error_from_node(&self.0, error)
    }
}

#[interface(name = "org.a11y.atspi.EditableText")]
impl EditableTextInterface {
    async fn set_text_contents(&self, new_contents: &str) -> fdo::Result<bool> {
        let change = self.0.value_change().map_err(self.map_error())?;
        let done = self
            .0
            .set_text_contents(new_contents)
            .map_err(self.map_error())?;
        if done && !change.holds_text(new_contents) {
            settle(change).await;
        }
        Ok(done)
    }

    async fn insert_text(&self, position: i32, text: &str, length: i32) -> fdo::Result<bool> {
        let change = self.0.value_change().map_err(self.map_error())?;
        let done = self
            .0
            .insert_text(position, text, length)
            .map_err(self.map_error())?;
        if done && !text.is_empty() && length != 0 {
            settle(change).await;
        }
        Ok(done)
    }

    fn copy_text(&self, _start_pos: i32, _end_pos: i32) -> fdo::Result<()> {
        Err(unsupported())
    }

    fn cut_text(&self, _start_pos: i32, _end_pos: i32) -> fdo::Result<bool> {
        Err(unsupported())
    }

    fn delete_text(&self, _start_pos: i32, _end_pos: i32) -> fdo::Result<bool> {
        Err(unsupported())
    }

    fn paste_text(&self, _position: i32) -> fdo::Result<bool> {
        Err(unsupported())
    }
}
