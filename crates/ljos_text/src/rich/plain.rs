//! Plain text as a document: one block, the text as it is.
//!
//! It stays one block because everything Qt does to plain text (eliding,
//! `maximumLineCount`, wrapping) works across its lines.

use super::{Block, BlockKind};

pub(crate) fn parse(source: &str) -> Vec<Block> {
    let mut block = Block::new(BlockKind::Paragraph);
    block.text = source.to_owned();
    block.seal();
    vec![block]
}
