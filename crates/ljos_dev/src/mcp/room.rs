//! The room left in one copy of an answer while its rows are written.
//!
//! A producer that lists what a client asked for stops at the budget and
//! says where to go on, instead of building more than it may send and
//! leaving the cutting to the end, which would drop rows its `next_since`
//! already counts as told.

use ljos_mcp_core::cap_result;
use serde_json::Value;

/// What the counts written after the rows may take.
const SLACK: usize = 1024;

#[derive(Debug)]
pub(crate) struct Room {
    json: usize,
    text: usize,
}

fn size_of(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(0, |v| v.len())
}

impl Room {
    /// The room of `per_copy` bytes once the rest of the answer, `base`
    /// and `base_text`, is in.
    pub(crate) fn new(per_copy: usize, base: &Value, base_text: &str) -> Room {
        Room {
            json: per_copy.saturating_sub(size_of(base) + SLACK),
            text: per_copy.saturating_sub(base_text.len() + SLACK),
        }
    }

    /// Takes the room of `row` and its `line`, if both fit.
    pub(crate) fn take(&mut self, row: &Value, line: &str) -> bool {
        // A comma between rows and a line break after each line.
        let (json, text) = (size_of(row) + 1, line.len() + 1);
        if json > self.json || text > self.text {
            return false;
        }
        self.json -= json;
        self.text -= text;
        true
    }

    /// `row` and `line` cut to what is left, for a first row that does not
    /// fit on its own: an answer always moves its reader on by one.
    pub(crate) fn squeeze(&mut self, row: Value, line: &str) -> (Value, String) {
        let row = cap_result(row, self.json.saturating_sub(1).max(64));
        let mut line = line.to_owned();
        let most = self.text.saturating_sub(1).max(64);
        if line.len() > most {
            line.truncate(line.floor_char_boundary(most.saturating_sub(1)));
            line.push('…');
        }
        self.json = self.json.saturating_sub(size_of(&row) + 1);
        self.text = self.text.saturating_sub(line.len() + 1);
        (row, line)
    }
}

/// The rows of `items` that fit in `room`, in their order: the last ones
/// with `newest` (an answer without `since`), else the first ones. A first
/// row too big on its own is squeezed in.
pub(crate) fn rows<'a, T>(
    room: &mut Room,
    items: &[&'a T],
    newest: bool,
    row: impl Fn(&T) -> (Value, String),
) -> Vec<(&'a T, Value, String)> {
    let order: Box<dyn Iterator<Item = &&'a T>> = if newest {
        Box::new(items.iter().rev())
    } else {
        Box::new(items.iter())
    };
    let mut out = Vec::new();
    for &item in order {
        let (v, line) = row(item);
        let (v, line) = if room.take(&v, &line) {
            (v, line)
        } else if out.is_empty() {
            room.squeeze(v, &line)
        } else {
            break;
        };
        out.push((item, v, line));
    }
    if newest {
        out.reverse();
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn rows_are_taken_until_the_room_is_full() {
        let mut room = Room::new(2048, &json!({}), "");
        let row = json!({ "seq": 1, "value": "x".repeat(100) });
        let taken = (0..100).take_while(|_| room.take(&row, "a line")).count();
        assert!(taken > 0 && taken < 100);
    }

    #[test]
    fn the_newest_rows_are_kept_without_since() {
        let items: Vec<u32> = (0..1000).collect();
        let refs: Vec<&u32> = items.iter().collect();
        let mut room = Room::new(4096, &json!({}), "");
        let kept = rows(&mut room, &refs, true, |i| {
            (json!({ "seq": i }), format!("row {i}"))
        });
        assert!(kept.len() < 1000);
        assert_eq!(*kept.last().unwrap().0, 999);
        let mut room = Room::new(4096, &json!({}), "");
        let kept = rows(&mut room, &refs, false, |i| {
            (json!({ "seq": i }), format!("row {i}"))
        });
        assert_eq!(*kept[0].0, 0);
    }

    #[test]
    fn a_row_too_big_for_the_room_is_squeezed_into_it() {
        let mut room = Room::new(2048, &json!({}), "");
        let row = json!({ "seq": 1, "value": "x".repeat(100_000) });
        let line = "y".repeat(100_000);
        assert!(!room.take(&row, &line));
        let (row, line) = room.squeeze(row, &line);
        assert!(size_of(&row) <= 2048 && line.len() <= 2048);
        assert_eq!(row["seq"], 1);
    }
}
