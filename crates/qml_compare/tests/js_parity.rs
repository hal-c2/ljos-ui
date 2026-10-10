//! Keeps the Rust metric in step with the web gallery's `compare.js`.
//!
//! The pictures are synthetic (gradients, shapes, translucent edges, noise)
//! so no Qt-derived image is needed. The JavaScript is run unmodified from a
//! qml-solid checkout (`QML_SOLID_DIR`, default `~/dev/qml-solid`); the test
//! is skipped, loudly, when node or that checkout is missing.

use std::path::PathBuf;
use std::process::Command;

use qml_compare::image::Rgba;
use qml_compare::metric::{DEFAULT_THRESHOLD, compare};

/// xorshift: deterministic noise without a dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 32) as u32
    }
}

fn scene(w: u32, h: u32, translucent: bool) -> Rgba {
    let mut data = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let (cx, cy) = (x as f64 - 40.0, y as f64 - 30.0);
            let d = (cx * cx + cy * cy).sqrt();
            // A soft-edged disc over a flat background, a bar, and a ramp.
            let mut p = [250u8, 248, 240, 255];
            if d < 22.0 {
                let t = ((22.0 - d) / 2.0).clamp(0.0, 1.0);
                p = [
                    (250.0 - 200.0 * t) as u8,
                    (248.0 - 120.0 * t) as u8,
                    (240.0 - 40.0 * t) as u8,
                    if translucent {
                        (120.0 + 135.0 * t) as u8
                    } else {
                        255
                    },
                ];
            }
            if (60..64).contains(&x) && y > 10 {
                p = [20, 30, 200, 255];
            }
            if y > h - 12 {
                p = [(x * 255 / w) as u8, 90, 90, 255];
            }
            data.extend_from_slice(&p);
        }
    }
    Rgba::new(w, h, data).unwrap()
}

fn perturbed(base: &Rgba, kind: usize) -> Rgba {
    let mut out = base.clone();
    let w = base.width as usize;
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 + kind as u64);
    match kind {
        // Identical.
        0 => {}
        // Shifted one pixel: edges everywhere, mostly anti-aliasing.
        1 => {
            for y in 0..base.height as usize {
                for x in 1..w {
                    let (d, s) = (4 * (y * w + x), 4 * (y * w + x - 1));
                    out.data[d..d + 4].copy_from_slice(&base.data[s..s + 4]);
                }
            }
        }
        // A block of clearly different colour.
        2 => {
            for y in 20..40 {
                for x in 70..90 {
                    let i = 4 * (y * w + x);
                    out.data[i..i + 3].copy_from_slice(&[200, 40, 40]);
                }
            }
        }
        // Sparse noise of every size, including sub-threshold.
        3 => {
            for _ in 0..600 {
                let i = 4 * (rng.next() as usize % (out.data.len() / 4));
                let amount = (rng.next() % 90) as i32 - 45;
                for c in 0..3 {
                    out.data[i + c] = (i32::from(out.data[i + c]) + amount).clamp(0, 255) as u8;
                }
                if rng.next().is_multiple_of(5) {
                    out.data[i + 3] = (rng.next() % 256) as u8;
                }
            }
        }
        // Everything slightly off the background: tests the background rule.
        4 => {
            for p in out.data.as_chunks_mut::<4>().0.iter_mut() {
                p[0] = p[0].saturating_sub(7);
                p[2] = p[2].saturating_sub(7);
            }
        }
        // Everything noisy.
        _ => {
            for p in out.data.as_chunks_mut::<4>().0.iter_mut() {
                for c in &mut p[..3] {
                    *c = c.wrapping_add((rng.next() % 40) as u8);
                }
            }
        }
    }
    out
}

fn solid_dir() -> PathBuf {
    std::env::var_os("QML_SOLID_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join("dev/qml-solid")))
        .unwrap_or_default()
}

/// Score each (actual, reference) pair with both implementations.
fn assert_parity(gallery: PathBuf, cases: &[(Rgba, Rgba, PathBuf, PathBuf)]) -> Vec<f64> {
    let mut args = vec![
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/js/score.mjs"),
        gallery,
    ];
    for (_, _, a, r) in cases {
        args.push(a.clone());
        args.push(r.clone());
    }
    let run = Command::new("node").args(&args).output().unwrap();
    assert!(
        run.status.success(),
        "node failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let js: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    for (n, (actual, reference, ..)) in cases.iter().enumerate() {
        let rust = compare(actual, reference, DEFAULT_THRESHOLD).unwrap();
        let (pixels, content) = (
            js[n]["pixels"].as_f64().unwrap(),
            js[n]["content"].as_f64().unwrap(),
        );
        assert!(
            (rust.pixels - pixels).abs() < 1e-6,
            "case {n}: pixels {} vs js {pixels}",
            rust.pixels
        );
        assert!(
            (rust.content - content).abs() < 1e-6,
            "case {n}: content {} vs js {content}",
            rust.content
        );
        let js_diff =
            Rgba::from_png(&decode_base64(js[n]["diffBase64"].as_str().unwrap())).unwrap();
        assert_eq!(rust.diff, js_diff, "case {n}: diff picture differs");
    }
    (0..cases.len())
        .map(|n| js[n]["pixels"].as_f64().unwrap())
        .collect()
}

fn available() -> Option<PathBuf> {
    let gallery = solid_dir().join("examples/gallery");
    if !gallery.join("tests/compare.js").is_file()
        || Command::new("node").arg("-v").output().is_err()
    {
        eprintln!(
            "SKIPPED: needs node and a qml-solid checkout at {} (QML_SOLID_DIR)",
            gallery.display()
        );
        return None;
    }
    Some(gallery)
}

/// Qt's reference pictures are never committed, but when a corpus checkout
/// has them, the real thing is the best test of all.
#[test]
fn agrees_with_compare_js_on_real_references() {
    let Some(gallery) = available() else { return };
    let references = solid_dir().join("corpus/reference");
    let Ok(entries) = std::fs::read_dir(&references) else {
        eprintln!("SKIPPED: no reference pictures in {}", references.display());
        return;
    };
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("js_parity_real");
    std::fs::create_dir_all(&dir).unwrap();
    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    paths.sort();
    let mut cases = Vec::new();
    for path in paths.iter().take(6) {
        let reference = Rgba::read(path).unwrap();
        for kind in [0, 1, 3] {
            let actual = perturbed(&reference, kind);
            let a = dir.join(format!(
                "{}-{kind}.png",
                path.file_stem().unwrap().to_string_lossy()
            ));
            actual.write(&a).unwrap();
            cases.push((actual, reference.clone(), a, path.clone()));
        }
    }
    assert_parity(gallery, &cases);
}

#[test]
fn agrees_with_compare_js() {
    let Some(gallery) = available() else { return };
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("js_parity");
    std::fs::create_dir_all(&dir).unwrap();

    let mut cases = Vec::new();
    for (index, translucent) in [false, true].into_iter().enumerate() {
        let reference = scene(100, 80, translucent);
        for kind in 0..6 {
            let actual = perturbed(&reference, kind);
            let (a, r) = (
                dir.join(format!("{index}-{kind}-a.png")),
                dir.join(format!("{index}-{kind}-r.png")),
            );
            actual.write(&a).unwrap();
            reference.write(&r).unwrap();
            cases.push((actual, reference.clone(), a, r));
        }
    }
    let spread = assert_parity(gallery, &cases);
    // The scenarios must actually exercise the metric.
    assert!(
        spread.contains(&1.0) && spread.iter().any(|&p| p < 0.99),
        "{spread:?}"
    );
}

fn decode_base64(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes().filter(|&c| c != b'=') {
        acc = acc << 6 | u32::from(value(c));
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}
