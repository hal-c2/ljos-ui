//! Which cell of a `GridLayout` each child sits in.
//!
//! The rules are Qt's, found by experiment with `qml6` and pinned by
//! `features/layouts/grid-layout.feature`.

use std::collections::HashSet;

/// What a child asks for: `Layout.row`, `Layout.column` and the spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Want {
    pub row: Option<usize>,
    pub column: Option<usize>,
    pub row_span: usize,
    pub column_span: usize,
}

/// Where a child ended up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
}

/// Places the children in order.
///
/// A child goes into the next free cell after the one before it, along the
/// flow: across a row until `columns` are full, or down a column until
/// `rows` are (`top_to_bottom`). Without that limit there is one row, or
/// one column. A child that names its row or column sits there (the other
/// one is 0 when it is not named), and the next child follows it.
pub(crate) fn place(
    wants: &[Want],
    rows: Option<usize>,
    columns: Option<usize>,
    top_to_bottom: bool,
) -> Vec<Slot> {
    // Worked out along the flow: `at` runs along a line, `line` counts them.
    let limit = if top_to_bottom { rows } else { columns }
        .filter(|n| *n > 0)
        .unwrap_or(usize::MAX);
    let mut taken: HashSet<(usize, usize)> = HashSet::new();
    let (mut line, mut at) = (0usize, 0usize);
    wants
        .iter()
        .map(|w| {
            let (want_line, want_at, across, along) = if top_to_bottom {
                (w.column, w.row, w.column_span, w.row_span)
            } else {
                (w.row, w.column, w.row_span, w.column_span)
            };
            let along = along.clamp(1, limit);
            let across = across.max(1);
            if want_line.is_some() || want_at.is_some() {
                line = want_line.unwrap_or(0);
                at = want_at.unwrap_or(0);
            } else {
                loop {
                    if at > 0 && at.saturating_add(along) > limit {
                        line += 1;
                        at = 0;
                    }
                    let free = (line..line + across)
                        .all(|l| (at..at + along).all(|a| !taken.contains(&(l, a))));
                    if free {
                        break;
                    }
                    at += 1;
                }
            }
            for l in line..line + across {
                for a in at..at + along {
                    taken.insert((l, a));
                }
            }
            let slot = if top_to_bottom {
                Slot {
                    row: at,
                    column: line,
                    row_span: along,
                    column_span: across,
                }
            } else {
                Slot {
                    row: line,
                    column: at,
                    row_span: across,
                    column_span: along,
                }
            };
            at += along;
            slot
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTO: Want = Want {
        row: None,
        column: None,
        row_span: 1,
        column_span: 1,
    };

    fn cells(slots: &[Slot]) -> Vec<(usize, usize)> {
        slots.iter().map(|s| (s.row, s.column)).collect()
    }

    #[test]
    fn rows_fill_up_to_the_column_limit() {
        let s = place(&[AUTO; 5], None, Some(3), false);
        assert_eq!(cells(&s), vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1)]);
    }

    #[test]
    fn top_to_bottom_fills_columns_up_to_the_row_limit() {
        let s = place(&[AUTO; 4], Some(3), None, true);
        assert_eq!(cells(&s), vec![(0, 0), (1, 0), (2, 0), (0, 1)]);
    }

    #[test]
    fn no_limit_is_one_line() {
        assert_eq!(
            cells(&place(&[AUTO; 2], None, None, false)),
            vec![(0, 0), (0, 1)]
        );
        assert_eq!(
            cells(&place(&[AUTO; 2], None, None, true)),
            vec![(0, 0), (1, 0)]
        );
        // The limit of the other flow does not count.
        assert_eq!(
            cells(&place(&[AUTO; 2], Some(1), None, false)),
            vec![(0, 0), (0, 1)]
        );
    }

    #[test]
    fn a_named_cell_moves_the_children_after_it() {
        let named = |row, column| Want {
            row,
            column,
            ..AUTO
        };
        let s = place(
            &[
                named(Some(1), Some(1)),
                AUTO,
                AUTO,
                named(None, Some(2)),
                AUTO,
            ],
            None,
            Some(3),
            false,
        );
        assert_eq!(cells(&s), vec![(1, 1), (1, 2), (2, 0), (0, 2), (1, 0)]);
    }

    #[test]
    fn a_span_wraps_and_taken_cells_are_skipped() {
        let wide = Want {
            column_span: 2,
            ..AUTO
        };
        let tall = Want {
            row_span: 2,
            ..AUTO
        };
        // a b / wide / c d
        let s = place(&[AUTO, AUTO, wide, AUTO, AUTO], None, Some(2), false);
        assert_eq!(cells(&s), vec![(0, 0), (0, 1), (1, 0), (2, 0), (2, 1)]);
        // tall a / tall b / c
        let s = place(&[tall, AUTO, AUTO, AUTO], None, Some(2), false);
        assert_eq!(cells(&s), vec![(0, 0), (0, 1), (1, 1), (2, 0)]);
    }
}
