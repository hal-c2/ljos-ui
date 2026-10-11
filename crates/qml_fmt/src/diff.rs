//! A unified diff of two texts, for showing what the formatter would
//! change without changing it.

use std::fmt::Write as _;

/// Lines of context around a change.
const CONTEXT: usize = 3;
/// Above this many cells the longest common subsequence is not worked out
/// and the differing middle is shown as one block: the diff is for reading,
/// and the table would cost more memory than a layout change is worth.
const MAX_CELLS: usize = 4_000_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Same,
    Remove,
    Add,
}

/// The edit script from `old` to `new`, a line at a time.
fn script<'t>(old: &[&'t str], new: &[&'t str]) -> Vec<(Op, &'t str)> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (a, b) = (
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    );
    let mut ops: Vec<(Op, &str)> = old[..prefix].iter().map(|l| (Op::Same, *l)).collect();
    if a.len().saturating_mul(b.len()) > MAX_CELLS {
        ops.extend(a.iter().map(|l| (Op::Remove, *l)));
        ops.extend(b.iter().map(|l| (Op::Add, *l)));
    } else {
        // lcs[i][j]: the longest common subsequence of a[i..] and b[j..].
        let width = b.len() + 1;
        let mut lcs = vec![0u32; (a.len() + 1) * width];
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                lcs[i * width + j] = if a[i] == b[j] {
                    lcs[(i + 1) * width + j + 1] + 1
                } else {
                    lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < a.len() && j < b.len() {
            if a[i] == b[j] {
                ops.push((Op::Same, a[i]));
                i += 1;
                j += 1;
            } else if lcs[(i + 1) * width + j] >= lcs[i * width + j + 1] {
                ops.push((Op::Remove, a[i]));
                i += 1;
            } else {
                ops.push((Op::Add, b[j]));
                j += 1;
            }
        }
        ops.extend(a[i..].iter().map(|l| (Op::Remove, *l)));
        ops.extend(b[j..].iter().map(|l| (Op::Add, *l)));
    }
    ops.extend(old[old.len() - suffix..].iter().map(|l| (Op::Same, *l)));
    ops
}

fn range(start: usize, count: usize) -> String {
    match count {
        // By convention an empty range names the line before it.
        0 => format!("{},0", start.saturating_sub(1)),
        1 => start.to_string(),
        _ => format!("{start},{count}"),
    }
}

/// The unified diff from `old` to `new` with three lines of context, under
/// the headers `--- a/{name}` and `+++ b/{name}`. Empty when the texts are
/// equal.
pub fn unified_diff(name: &str, old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new.split_inclusive('\n').collect();
    let ops = script(&old_lines, &new_lines);

    let mut out = format!("--- a/{name}\n+++ b/{name}\n");
    let changed: Vec<usize> = (0..ops.len()).filter(|i| ops[*i].0 != Op::Same).collect();
    let mut at = 0;
    while at < changed.len() {
        // A hunk: changes whose contexts touch.
        let first = changed[at];
        let mut last = first;
        while at + 1 < changed.len() && changed[at + 1] <= last + 2 * CONTEXT + 1 {
            at += 1;
            last = changed[at];
        }
        at += 1;
        let from = first.saturating_sub(CONTEXT);
        let to = (last + CONTEXT + 1).min(ops.len());
        let before = |op: Op| ops[..from].iter().filter(|o| o.0 != op).count();
        let within = |op: Op| ops[from..to].iter().filter(|o| o.0 != op).count();
        let _ = writeln!(
            out,
            "@@ -{} +{} @@",
            range(before(Op::Add) + 1, within(Op::Add)),
            range(before(Op::Remove) + 1, within(Op::Remove)),
        );
        for (op, line) in &ops[from..to] {
            out.push(match op {
                Op::Same => ' ',
                Op::Remove => '-',
                Op::Add => '+',
            });
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_texts_have_no_diff() {
        assert_eq!(unified_diff("a.qml", "x\n", "x\n"), "");
    }

    #[test]
    fn a_change_is_shown_with_its_context() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n9\n";
        let new = "1\n2\n3\n4\nfive\n6\n7\n8\n9\n";
        assert_eq!(
            unified_diff("a.qml", old, new),
            "--- a/a.qml\n+++ b/a.qml\n@@ -2,7 +2,7 @@\n 2\n 3\n 4\n-5\n+five\n 6\n 7\n 8\n"
        );
    }

    #[test]
    fn far_changes_get_a_hunk_each_and_near_ones_share_one() {
        let old: String = (1..=30).map(|n| format!("{n}\n")).collect();
        let far = old
            .replace("\n2\n", "\ntwo\n")
            .replace("\n20\n", "\ntwenty\n");
        assert_eq!(unified_diff("f", &old, &far).matches("@@ -").count(), 2);
        let near = old
            .replace("\n2\n", "\ntwo\n")
            .replace("\n8\n", "\neight\n");
        assert_eq!(unified_diff("f", &old, &near).matches("@@ -").count(), 1);
    }

    #[test]
    fn added_and_removed_lines_move_the_ranges() {
        let diff = unified_diff("f", "a\nb\n", "a\nx\ny\nb\n");
        assert_eq!(diff, "--- a/f\n+++ b/f\n@@ -1,2 +1,4 @@\n a\n+x\n+y\n b\n");
        let diff = unified_diff("f", "", "a\n");
        assert_eq!(diff, "--- a/f\n+++ b/f\n@@ -0,0 +1 @@\n+a\n");
    }

    #[test]
    fn a_missing_last_line_end_is_said() {
        let diff = unified_diff("f", "a", "a\n");
        assert_eq!(
            diff,
            "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+a\n"
        );
    }

    #[test]
    fn a_huge_difference_is_one_block() {
        let old: String = (0..3000).map(|n| format!("a{n}\n")).collect();
        let new: String = (0..3000).map(|n| format!("b{n}\n")).collect();
        let diff = unified_diff("f", &old, &new);
        assert_eq!(diff.matches("@@ -").count(), 1);
        assert_eq!(diff.lines().filter(|l| l.starts_with('-')).count(), 3001);
    }
}
