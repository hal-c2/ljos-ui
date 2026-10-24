//! Greedy shrinking: of the smaller cases an area offers, take the first
//! that still shows the difference, and go on from it until none does.

use std::time::{Duration, Instant};

use crate::area::Area;

/// Limits on one shrink, so a run with many buckets still ends.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub rounds: usize,
    pub time: Duration,
    /// Candidates judged together: both sides start once per chunk.
    pub chunk: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            rounds: 300,
            time: Duration::from_secs(90),
            chunk: 48,
        }
    }
}

/// The smallest case found that `keeps` says still shows the difference.
/// `keeps` judges a chunk of candidates at once and answers for each.
pub fn shrink<A: Area>(
    area: &A,
    case: &A::Case,
    budget: Budget,
    keeps: &mut dyn FnMut(&[A::Case]) -> Vec<bool>,
) -> A::Case {
    let start = Instant::now();
    let mut current = case.clone();
    'rounds: for _ in 0..budget.rounds {
        let candidates = area.reductions(&current);
        for chunk in candidates.chunks(budget.chunk.max(1)) {
            if start.elapsed() > budget.time {
                break 'rounds;
            }
            if let Some(i) = keeps(chunk).iter().position(|k| *k) {
                current = chunk[i].clone();
                continue 'rounds;
            }
        }
        break;
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bindings::{Bindings, ast::Program},
        rng::Rng,
    };

    #[test]
    fn a_property_kept_alive_is_all_that_is_left() {
        let area = Bindings;
        let p = (0..50)
            .map(|n| area.generate(&mut Rng::for_case(9, n)))
            .max_by_key(Program::size)
            .expect("some programs");
        let target = p.root.props.first().map(|p| p.name.clone());
        let Some(target) = target else { return };
        let small = shrink(&area, &p, Budget::default(), &mut |c| {
            c.iter()
                .map(|c| c.root.props.iter().any(|p| p.name == target))
                .collect()
        });
        assert!(small.size() < p.size());
        assert_eq!(small.root.all().len(), 1);
        assert!(small.steps.is_empty());
        assert_eq!(small.root.props.len(), 1, "{}", area.program(&small));
    }
}
