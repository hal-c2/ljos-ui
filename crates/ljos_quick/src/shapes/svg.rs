//! The path data of SVG (`PathSvg.path`): the whole grammar of SVG 1.1
//! section 8.3, with the flags of an arc allowed to stand without anything
//! between them as SVG says they may.
//!
//! What can be read before a mistake is kept, as Qt keeps it, and the
//! mistake is told with the place it is at, which Qt does not do.

use ljos_render::kurbo::{Point, Vec2};

/// One step of path data, in the coordinates the data counts in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SvgEl {
    Move(Point),
    Line(Point),
    Quad(Point, Point),
    Cubic(Point, Point, Point),
    Arc {
        radii: Vec2,
        /// In degrees, as written.
        rotation: f64,
        large: bool,
        sweep: bool,
        to: Point,
    },
    Close,
}

/// Why path data could not be read to its end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SvgError {
    /// What is wrong, ending in the place: `unexpected 'o' at character 12`.
    pub message: String,
}

/// How many numbers one group of `cmd` takes.
fn arity(cmd: char) -> Option<usize> {
    Some(match cmd.to_ascii_uppercase() {
        'M' | 'L' | 'T' => 2,
        'H' | 'V' => 1,
        'C' => 6,
        'S' | 'Q' => 4,
        'A' => 7,
        'Z' => 0,
        _ => return None,
    })
}

struct Reader<'a> {
    chars: Vec<char>,
    at: usize,
    _data: &'a str,
}

impl Reader<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    /// The place of the next character, counted from one as an editor
    /// counts columns.
    fn place(&self) -> usize {
        self.at + 1
    }

    fn skip_space(&mut self) {
        while self
            .peek()
            .is_some_and(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0c'))
        {
            self.at += 1;
        }
    }

    /// Space, with at most one comma in it.
    fn skip_separator(&mut self) {
        self.skip_space();
        if self.peek() == Some(',') {
            self.at += 1;
            self.skip_space();
        }
    }

    fn starts_number(&self) -> bool {
        self.peek()
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.'))
    }

    /// A number as SVG writes it: a sign, digits with at most one point,
    /// and an exponent. It ends where the next character cannot belong to
    /// it, so `1.5.5` is two numbers and `10-20` is two.
    fn number(&mut self) -> Option<f64> {
        let start = self.at;
        let mut i = self.at;
        let at = |i: usize| self.chars.get(i).copied();
        if matches!(at(i), Some('+' | '-')) {
            i += 1;
        }
        let mut digits = 0;
        while at(i).is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
            digits += 1;
        }
        if at(i) == Some('.') {
            i += 1;
            while at(i).is_some_and(|c| c.is_ascii_digit()) {
                i += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return None;
        }
        // An `e` with no digits after it is not an exponent.
        if matches!(at(i), Some('e' | 'E')) {
            let mut j = i + 1;
            if matches!(at(j), Some('+' | '-')) {
                j += 1;
            }
            if at(j).is_some_and(|c| c.is_ascii_digit()) {
                while at(j).is_some_and(|c| c.is_ascii_digit()) {
                    j += 1;
                }
                i = j;
            }
        }
        let text: String = self.chars[start..i].iter().collect();
        let n = text.parse::<f64>().ok()?;
        self.at = i;
        Some(n)
    }
}

/// Reads path data. The steps read before a mistake are returned with it.
pub(crate) fn parse(data: &str) -> (Vec<SvgEl>, Option<SvgError>) {
    let mut out = Vec::new();
    let error = read(data, &mut out)
        .err()
        .map(|message| SvgError { message });
    (out, error)
}

fn read(data: &str, out: &mut Vec<SvgEl>) -> Result<(), String> {
    let mut r = Reader {
        chars: data.chars().collect(),
        at: 0,
        _data: data,
    };
    // The data's own current point and the start of its current part: both
    // at the origin until the data says otherwise, wherever the path is.
    let mut cur = Point::ZERO;
    let mut start = Point::ZERO;
    // The control point a smooth curve mirrors, when the step before was a
    // curve of its kind.
    let mut cubic_ctrl: Option<Point> = None;
    let mut quad_ctrl: Option<Point> = None;
    // The command a bare group of numbers repeats.
    let mut repeat: Option<char> = None;
    loop {
        r.skip_space();
        let Some(c) = r.peek() else {
            return Ok(());
        };
        let group = r.place();
        let cmd = if c.is_ascii_alphabetic() {
            if arity(c).is_none() {
                return Err(format!("unknown command '{c}' at character {group}"));
            }
            r.at += 1;
            c
        } else if r.starts_number() {
            match repeat {
                Some(cmd) => cmd,
                None => {
                    return Err(format!(
                        "a number where a command should be, at character {group}"
                    ));
                }
            }
        } else {
            return Err(format!("unexpected '{c}' at character {group}"));
        };
        let n = arity(cmd).unwrap_or(0);
        let mut a = [0.0_f64; 7];
        for (k, slot) in a.iter_mut().enumerate().take(n) {
            if k == 0 {
                r.skip_space();
            } else {
                r.skip_separator();
            }
            let short = || format!("'{cmd}' needs {n} numbers, found {k}, at character {group}");
            let Some(c) = r.peek() else {
                return Err(short());
            };
            if c.is_ascii_alphabetic() && arity(c).is_some() {
                return Err(short());
            }
            let flag = cmd.eq_ignore_ascii_case(&'A') && (k == 3 || k == 4);
            if flag {
                // One character, so that `0180` is two flags and a number.
                *slot = match c {
                    '0' => 0.0,
                    '1' => 1.0,
                    _ if r.starts_number() => {
                        return Err(format!(
                            "an arc flag must be 0 or 1, at character {}",
                            r.place()
                        ));
                    }
                    _ => return Err(format!("unexpected '{c}' at character {}", r.place())),
                };
                r.at += 1;
                continue;
            }
            let place = r.place();
            match r.number() {
                Some(v) => *slot = v,
                None => return Err(format!("unexpected '{c}' at character {place}")),
            }
        }
        let relative = cmd.is_ascii_lowercase();
        let point = |x: f64, y: f64| {
            if relative {
                Point::new(cur.x + x, cur.y + y)
            } else {
                Point::new(x, y)
            }
        };
        let (mut next_cubic, mut next_quad) = (None, None);
        match cmd.to_ascii_uppercase() {
            'M' => {
                let p = point(a[0], a[1]);
                out.push(SvgEl::Move(p));
                cur = p;
                start = p;
            }
            'L' => {
                let p = point(a[0], a[1]);
                out.push(SvgEl::Line(p));
                cur = p;
            }
            'H' => {
                let p = Point::new(if relative { cur.x + a[0] } else { a[0] }, cur.y);
                out.push(SvgEl::Line(p));
                cur = p;
            }
            'V' => {
                let p = Point::new(cur.x, if relative { cur.y + a[0] } else { a[0] });
                out.push(SvgEl::Line(p));
                cur = p;
            }
            'C' => {
                let (c1, c2, p) = (point(a[0], a[1]), point(a[2], a[3]), point(a[4], a[5]));
                out.push(SvgEl::Cubic(c1, c2, p));
                next_cubic = Some(c2);
                cur = p;
            }
            'S' => {
                let c1 = cubic_ctrl.map_or(cur, |c| cur + (cur - c));
                let (c2, p) = (point(a[0], a[1]), point(a[2], a[3]));
                out.push(SvgEl::Cubic(c1, c2, p));
                next_cubic = Some(c2);
                cur = p;
            }
            'Q' => {
                let (c, p) = (point(a[0], a[1]), point(a[2], a[3]));
                out.push(SvgEl::Quad(c, p));
                next_quad = Some(c);
                cur = p;
            }
            'T' => {
                let c = quad_ctrl.map_or(cur, |c| cur + (cur - c));
                let p = point(a[0], a[1]);
                out.push(SvgEl::Quad(c, p));
                next_quad = Some(c);
                cur = p;
            }
            'A' => {
                let p = point(a[5], a[6]);
                out.push(SvgEl::Arc {
                    radii: Vec2::new(a[0], a[1]),
                    rotation: a[2],
                    large: a[3] != 0.0,
                    sweep: a[4] != 0.0,
                    to: p,
                });
                cur = p;
            }
            _ => {
                out.push(SvgEl::Close);
                cur = start;
            }
        }
        cubic_ctrl = next_cubic;
        quad_ctrl = next_quad;
        // Points after a move are lines; nothing repeats a close.
        repeat = match cmd {
            'M' => Some('L'),
            'm' => Some('l'),
            'Z' | 'z' => None,
            other => Some(other),
        };
        // One comma may end a group, before the next or before nothing.
        r.skip_separator();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn ok(data: &str) -> Vec<SvgEl> {
        let (els, err) = parse(data);
        assert_eq!(err, None, "{data}");
        els
    }

    fn fails(data: &str) -> (Vec<SvgEl>, String) {
        let (els, err) = parse(data);
        (els, err.expect("a mistake").message)
    }

    #[test]
    fn moves_and_lines() {
        let want = vec![
            SvgEl::Move(p(10.0, 20.0)),
            SvgEl::Line(p(60.0, 20.0)),
            SvgEl::Line(p(60.0, 80.0)),
        ];
        for data in [
            "M 10 20 L 60 20 L 60 80",
            "M10,20L60,20,60,80",
            "M 10 20 60 20 60 80",
            "m 10 20 50 0 0 60",
            "M 10 20 l 50 0 l 0 60",
            "M 10 20 H 60 V 80",
            "M 10 20 h 50 v 60",
            "M 10 20\n\tL 60 20\r\n L 60 80",
            "M 10 20 L 60 20 L 60 80,",
        ] {
            assert_eq!(ok(data), want, "{data}");
        }
    }

    #[test]
    fn numbers_end_where_they_must() {
        assert_eq!(
            ok("M10-20L-30-40 1e1.5z"),
            vec![
                SvgEl::Move(p(10.0, -20.0)),
                SvgEl::Line(p(-30.0, -40.0)),
                SvgEl::Line(p(10.0, 0.5)),
                SvgEl::Close,
            ]
        );
        assert_eq!(
            ok("M.5.5L5.+2e-1"),
            vec![SvgEl::Move(p(0.5, 0.5)), SvgEl::Line(p(5.0, 0.2))]
        );
    }

    #[test]
    fn repeated_horizontal_and_vertical() {
        assert_eq!(
            ok("M 10 20 h 20 30 v 30 30"),
            vec![
                SvgEl::Move(p(10.0, 20.0)),
                SvgEl::Line(p(30.0, 20.0)),
                SvgEl::Line(p(60.0, 20.0)),
                SvgEl::Line(p(60.0, 50.0)),
                SvgEl::Line(p(60.0, 80.0)),
            ]
        );
    }

    #[test]
    fn smooth_curves_mirror_the_control_point_before() {
        assert_eq!(
            ok("M 0 0 C 0 10 10 20 20 20 S 40 0 40 20"),
            vec![
                SvgEl::Move(p(0.0, 0.0)),
                SvgEl::Cubic(p(0.0, 10.0), p(10.0, 20.0), p(20.0, 20.0)),
                SvgEl::Cubic(p(30.0, 20.0), p(40.0, 0.0), p(40.0, 20.0)),
            ]
        );
        // With no curve before it, the current point stands in.
        assert_eq!(
            ok("M 5 5 S 10 0 20 5 T 30 5"),
            vec![
                SvgEl::Move(p(5.0, 5.0)),
                SvgEl::Cubic(p(5.0, 5.0), p(10.0, 0.0), p(20.0, 5.0)),
                SvgEl::Quad(p(20.0, 5.0), p(30.0, 5.0)),
            ]
        );
        assert_eq!(
            ok("M 0 0 Q 10 10 20 0 T 40 0 t 20 0"),
            vec![
                SvgEl::Move(p(0.0, 0.0)),
                SvgEl::Quad(p(10.0, 10.0), p(20.0, 0.0)),
                SvgEl::Quad(p(30.0, -10.0), p(40.0, 0.0)),
                SvgEl::Quad(p(50.0, 10.0), p(60.0, 0.0)),
            ]
        );
    }

    #[test]
    fn arc_flags_need_nothing_between_them() {
        let arc = |large, sweep, to| SvgEl::Arc {
            radii: Vec2::new(40.0, 40.0),
            rotation: 0.0,
            large,
            sweep,
            to,
        };
        assert_eq!(
            ok("M 10 50 A 40 40 0 01 90 50"),
            vec![SvgEl::Move(p(10.0, 50.0)), arc(false, true, p(90.0, 50.0))]
        );
        assert_eq!(
            ok("M10 50a40 40 0 0180 0"),
            vec![SvgEl::Move(p(10.0, 50.0)), arc(false, true, p(90.0, 50.0))]
        );
        assert_eq!(
            ok("M10 50a40 40 0 1 0 80 0 40 40 0 0 0 -80 0"),
            vec![
                SvgEl::Move(p(10.0, 50.0)),
                arc(true, false, p(90.0, 50.0)),
                arc(false, false, p(10.0, 50.0)),
            ]
        );
    }

    #[test]
    fn a_close_goes_back_to_the_start_of_the_part() {
        assert_eq!(
            ok("M 10 10 l 10 0 l 0 10 z l 5 5 Z"),
            vec![
                SvgEl::Move(p(10.0, 10.0)),
                SvgEl::Line(p(20.0, 10.0)),
                SvgEl::Line(p(20.0, 20.0)),
                SvgEl::Close,
                SvgEl::Line(p(15.0, 15.0)),
                SvgEl::Close,
            ]
        );
    }

    #[test]
    fn data_without_a_move_counts_from_the_origin() {
        assert_eq!(ok("l 10 10"), vec![SvgEl::Line(p(10.0, 10.0))]);
        assert_eq!(ok(""), vec![]);
        assert_eq!(ok("  \n "), vec![]);
    }

    #[test]
    fn mistakes_are_told_with_their_place() {
        let cases = [
            (
                "M 10 20 L 60 20 X 5 5 L 90 90",
                2,
                "unknown command 'X' at character 17",
            ),
            (
                "M 10 20 L 60 20 L 90",
                2,
                "'L' needs 2 numbers, found 1, at character 17",
            ),
            (
                "M 10 50 C 10 10 90 10",
                1,
                "'C' needs 6 numbers, found 4, at character 9",
            ),
            (
                "M 10 20 60 20 60",
                2,
                "'L' needs 2 numbers, found 1, at character 15",
            ),
            ("M 10 20 L 6o 20", 1, "unexpected 'o' at character 12"),
            ("M 10 20 L 60,,80", 1, "unexpected ',' at character 14"),
            (
                "10 20 L 60 20",
                0,
                "a number where a command should be, at character 1",
            ),
            (
                "M 10 50 A 40 40 0 2 1 90 50",
                1,
                "an arc flag must be 0 or 1, at character 19",
            ),
            (
                "M 10 20 L 60 20 L 70 30 Q 1",
                3,
                "'Q' needs 4 numbers, found 1, at character 25",
            ),
            (
                "M 10 20 L 6 L 20 20",
                1,
                "'L' needs 2 numbers, found 1, at character 9",
            ),
            (
                "M 10 20 z 5 5",
                2,
                "a number where a command should be, at character 11",
            ),
            // One comma after a group is let through, as Qt lets it.
            ("M 10 20 ,, L 5 5", 1, "unexpected ',' at character 10"),
        ];
        for (data, kept, message) in cases {
            let (els, got) = fails(data);
            assert_eq!(got, message, "{data}");
            assert_eq!(els.len(), kept, "{data}");
        }
    }
}
