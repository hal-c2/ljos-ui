//! `LodManager`: picks which level-of-detail child shows.

use ljos::prelude::*;

use crate::node::{Node, Object3D};
use crate::space;

/// Shows the child its camera's distance picks and hides the others,
/// writing their `visible` and `opacity`. The distance is to the manager's
/// own place, where its levels stand. Across a distance the two levels
/// meet over `fadeDistance`, centred on it, each shown at the opacity of
/// its share. Without a camera the children are left as declared.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Helpers", name = "LodManager", extends = Node)]
pub struct LodManager {
    #[qml(base)]
    node: Node,
    #[qml(notify)]
    camera: Option<ObjectId>,
    /// Distances from the camera at which the next level takes over.
    #[qml(notify)]
    distances: Vec<f64>,
    #[qml(notify)]
    fade_distance: f64,
}

impl Default for LodManager {
    fn default() -> Self {
        LodManager {
            node: Node::default(),
            camera: None,
            distances: Vec::new(),
            fade_distance: 0.0,
        }
    }
}

#[qml_impl]
impl LodManager {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        // Woken by the camera or the manager moving, its distances, or its
        // children changing; what it writes on the children wakes nothing.
        cx.engine.effect_in(me, move |e| {
            let Some(camera) = e.get(me, LodManager::CAMERA).ok().flatten() else {
                return;
            };
            let from = space::place(e, camera).world.translation();
            let at = space::place(e, me).world.translation();
            let distance = f64::from((from - at).length());
            let distances = e.get(me, LodManager::DISTANCES).unwrap_or_default();
            let fade = e.get(me, LodManager::FADE_DISTANCE).unwrap_or_default();
            let children: Vec<ObjectId> = match e.read(me, Object3D::DATA.index) {
                Value::List(l) => l
                    .iter()
                    .filter_map(Value::as_object)
                    .filter(|c| e.is::<Node>(*c))
                    .collect(),
                _ => Vec::new(),
            };
            let shares = shares(distance, &distances, fade, children.len());
            e.untrack(|| {
                for (child, share) in children.into_iter().zip(shares) {
                    let shown = share > 0.0;
                    if e.get(child, Node::VISIBLE).is_ok_and(|v| v != shown) {
                        crate::node::host_write(e, child, Node::VISIBLE, shown);
                    }
                    if shown && e.get(child, Node::OPACITY).is_ok_and(|o| o != share) {
                        crate::node::host_write(e, child, Node::OPACITY, share);
                    }
                }
            });
        })?;
        Ok(())
    }
}

/// How much of each of `count` levels shows at `distance`: one level, or
/// two whose shares add up to one inside a fade. A level past the last
/// child is shown by the last child.
fn shares(distance: f64, distances: &[f64], fade: f64, count: usize) -> Vec<f64> {
    let mut out = vec![0.0; count];
    let Some(last) = count.checked_sub(1) else {
        return out;
    };
    let level = distances.iter().take_while(|d| distance >= **d).count();
    out[level.min(last)] = 1.0;
    if fade <= 0.0 {
        return out;
    }
    let half = fade / 2.0;
    // The distance it fades across: the one just passed or the next one.
    let near = (0..distances.len().min(last)).find(|i| (distance - distances[*i]).abs() < half);
    if let Some(i) = near {
        let farther = ((distance - (distances[i] - half)) / fade).clamp(0.0, 1.0);
        out.fill(0.0);
        out[i] = 1.0 - farther;
        out[i + 1] = farther;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::shares;

    #[test]
    fn one_level_shows_out_of_a_fade() {
        let d = [400.0, 800.0];
        assert_eq!(shares(300.0, &d, 0.0, 3), [1.0, 0.0, 0.0]);
        assert_eq!(shares(400.0, &d, 0.0, 3), [0.0, 1.0, 0.0]);
        assert_eq!(shares(5000.0, &d, 0.0, 3), [0.0, 0.0, 1.0]);
        assert_eq!(shares(5000.0, &d, 0.0, 2), [0.0, 1.0]);
        assert!(shares(5000.0, &d, 0.0, 0).is_empty());
    }

    #[test]
    fn inside_a_fade_two_levels_share() {
        let d = [400.0, 800.0];
        assert_eq!(shares(425.0, &d, 100.0, 3), [0.25, 0.75, 0.0]);
        assert_eq!(shares(375.0, &d, 100.0, 3), [0.75, 0.25, 0.0]);
        assert_eq!(shares(600.0, &d, 100.0, 3), [0.0, 1.0, 0.0]);
        // No child past the last one to fade into.
        assert_eq!(shares(400.0, &d, 100.0, 1), [1.0]);
    }
}
