//! The smallest change this model says between its rows and new ones.
//!
//! The rule: rows equal at the start and at the end stay untouched. Between
//! them, the rows at the same index are said as changed, in only the roles
//! that differ, and what one side has more of is inserted or removed at the
//! end of that stretch. So one row edited is one `dataChanged`, one row put
//! in or taken out anywhere is one insertion or removal, and a move is said
//! as the rows between its two ends changing: finding moves would pair rows
//! by a guess at what identifies them, which JSON does not say.

use crate::parse::Row;

/// What to say, in this order: the rows from `first` for `changed` rows
/// (replaced in place), then `removed` rows or `inserted` rows at
/// `first + changed`.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub first: usize,
    pub changed: usize,
    pub removed: usize,
    pub inserted: usize,
}

/// `old` is borrowed row by row: the model keeps each with its key.
pub fn plan_refs(old: &[&Row], new: &[Row]) -> Plan {
    let first = old.iter().zip(new).take_while(|(o, n)| o.same(n)).count();
    // Only what the start did not take: a row counts once.
    let last = old[first..]
        .iter()
        .rev()
        .zip(new[first..].iter().rev())
        .take_while(|(o, n)| o.same(n))
        .count();
    let old_middle = old.len() - first - last;
    let new_middle = new.len() - first - last;
    let changed = old_middle.min(new_middle);
    Plan {
        first,
        changed,
        removed: old_middle - changed,
        inserted: new_middle - changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::rows;

    fn of(ns: &[i32]) -> Vec<Row> {
        let items: Vec<String> = ns.iter().map(|n| format!("{{\"n\":{n}}}")).collect();
        rows(format!("[{}]", items.join(",")).as_bytes()).unwrap()
    }

    fn says(old: &[i32], new: &[i32]) -> (usize, usize, usize, usize) {
        let old = of(old);
        let old: Vec<&Row> = old.iter().collect();
        let p = plan_refs(&old, &of(new));
        (p.first, p.changed, p.removed, p.inserted)
    }

    #[test]
    fn the_rule() {
        assert_eq!(says(&[1, 2, 3], &[1, 2, 3]), (3, 0, 0, 0));
        assert_eq!(says(&[], &[1, 2]), (0, 0, 0, 2));
        assert_eq!(says(&[1, 2], &[]), (0, 0, 2, 0));
        assert_eq!(says(&[1, 2, 3], &[1, 9, 3]), (1, 1, 0, 0));
        assert_eq!(says(&[1, 2, 3], &[1, 9, 2, 3]), (1, 0, 0, 1));
        assert_eq!(says(&[1, 2, 3], &[1, 3]), (1, 0, 1, 0));
        assert_eq!(says(&[1, 2, 3], &[9]), (0, 1, 2, 0));
        assert_eq!(says(&[1, 2], &[1, 8, 9]), (1, 1, 0, 1));
    }

    // A row equal at both ends is counted once.
    #[test]
    fn the_ends_do_not_overlap() {
        assert_eq!(says(&[1, 1], &[1, 1, 1]), (2, 0, 0, 1));
        assert_eq!(says(&[1, 1, 1], &[1, 1]), (2, 0, 1, 0));
    }

    #[test]
    fn a_move_is_said_as_the_rows_it_passes_changing() {
        assert_eq!(says(&[1, 2, 3, 4], &[1, 3, 2, 4]), (1, 2, 0, 0));
    }
}
