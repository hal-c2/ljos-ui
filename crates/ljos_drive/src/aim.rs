//! Where an input aimed at one item goes, and when there is nowhere.

use std::fmt;

use ljos_render::kurbo::{Point, Rect, Size};

/// Why an input aimed at an item is not sent: a person could not reach it
/// there either.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// It, or an item above it, has `enabled: false`: it takes no input.
    Disabled,
    /// It, or an item above it, has `visible: false`.
    Hidden,
    /// It has no area to put the pointer on.
    NoSize { width: f64, height: f64 },
    /// Its middle is outside the window.
    Outside { at: Point, window: Size },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => f.write_str("it is disabled, so it takes no input"),
            Self::Hidden => f.write_str("it is not shown"),
            Self::NoSize { width, height } => write!(f, "it has no size ({width}x{height})"),
            Self::Outside { at, window } => write!(
                f,
                "its middle is at {},{}, outside the window of {}x{}",
                at.x, at.y, window.width, window.height
            ),
        }
    }
}

/// The middle of `bounds` (in the window's coordinates), where a click on
/// the item goes, provided a person could click there.
pub fn aim(bounds: Rect, disabled: bool, window: Rect) -> Result<Point, Refusal> {
    if disabled {
        return Err(Refusal::Disabled);
    }
    if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return Err(Refusal::NoSize {
            width: bounds.width(),
            height: bounds.height(),
        });
    }
    aim_at_point(bounds.center(), window)
}

/// `at`, provided it is in the window: a person's pointer can be nowhere
/// else in it. The right and bottom edges are the next window's pixels.
pub fn aim_at_point(at: Point, window: Rect) -> Result<Point, Refusal> {
    if at.x < window.x0 || at.y < window.y0 || at.x >= window.x1 || at.y >= window.y1 {
        return Err(Refusal::Outside {
            at,
            window: window.size(),
        });
    }
    Ok(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Rect = Rect::new(0.0, 0.0, 100.0, 100.0);

    #[test]
    fn the_middle_of_what_can_be_reached() {
        let at = aim(Rect::new(10.0, 10.0, 30.0, 50.0), false, WINDOW);
        assert_eq!(at, Ok(Point::new(20.0, 30.0)));
    }

    #[test]
    fn a_point_is_in_the_window_up_to_but_not_on_its_far_edges() {
        for at in [Point::new(0.0, 0.0), Point::new(99.5, 99.5)] {
            assert_eq!(aim_at_point(at, WINDOW), Ok(at));
        }
        for at in [
            Point::new(-0.5, 0.0),
            Point::new(0.0, -0.5),
            Point::new(100.0, 0.0),
            Point::new(0.0, 100.0),
        ] {
            assert_eq!(
                aim_at_point(at, WINDOW),
                Err(Refusal::Outside {
                    at,
                    window: WINDOW.size()
                })
            );
        }
    }

    #[test]
    fn what_cannot_be_reached_is_refused() {
        let r = Rect::new(10.0, 10.0, 30.0, 50.0);
        assert_eq!(aim(r, true, WINDOW), Err(Refusal::Disabled));
        let flat = Rect::new(10.0, 10.0, 30.0, 10.0);
        assert!(matches!(
            aim(flat, false, WINDOW),
            Err(Refusal::NoSize { .. })
        ));
        let far = Rect::new(150.0, 10.0, 170.0, 30.0);
        assert!(matches!(
            aim(far, false, WINDOW),
            Err(Refusal::Outside { .. })
        ));
    }
}
