//! Which of a frame's draws are drawn together.
//!
//! A thousand cubes that differ only in where they are and in their colour
//! are one mesh drawn a thousand times with the same pipeline, maps and
//! uniforms but for those two. Drawn one by one, each costs a bind group,
//! buffers and a draw call to submit; drawn as the copies of one instanced
//! draw, with each one's place and colour in its copy's row, they cost one.
//! What is drawn together is decided by a key the caller gives each draw:
//! the same key, the same draw. A draw with no key is drawn on its own.

use std::collections::HashMap;
use std::hash::Hash;

/// The draws, by index, as they are to be drawn: each group where its
/// first draw is, its draws in the order they came. A key may stand for
/// more than draws alike, as a hash does: a draw is put with the first of
/// its key's group only where `alike` says the two are, and drawn alone
/// where not.
pub(crate) fn groups<K: Eq + Hash>(
    keys: impl IntoIterator<Item = Option<K>>,
    alike: impl Fn(usize, usize) -> bool,
) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut by_key: HashMap<K, usize> = HashMap::new();
    for (i, key) in keys.into_iter().enumerate() {
        match key {
            None => groups.push(vec![i]),
            Some(key) => match by_key.get(&key) {
                Some(&g) if alike(groups[g][0], i) => groups[g].push(i),
                Some(_) => groups.push(vec![i]),
                None => {
                    by_key.insert(key, groups.len());
                    groups.push(vec![i]);
                }
            },
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_with_one_key_are_one_group_where_the_first_of_them_is() {
        let keys = [
            Some('a'),
            Some('b'),
            Some('a'),
            None,
            Some('b'),
            None,
            Some('a'),
        ];
        assert_eq!(
            groups(keys, |_, _| true),
            [vec![0, 2, 6], vec![1, 4], vec![3], vec![5]]
        );
    }

    #[test]
    fn a_draw_without_a_key_is_drawn_alone() {
        assert_eq!(groups([None::<u8>, None], |_, _| true), [vec![0], vec![1]]);
    }

    #[test]
    fn a_draw_its_key_s_first_is_not_alike_is_drawn_alone() {
        let keys = [Some(1), Some(1), Some(1)];
        assert_eq!(groups(keys, |a, b| a + b != 1), [vec![0, 2], vec![1]]);
    }
}
