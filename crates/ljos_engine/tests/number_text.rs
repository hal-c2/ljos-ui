//! A number's text is the spec's shortest in both tiers: QuickJS (tier 2)
//! and `number_to_string` (tier 1) agree, and the digits are as few as
//! round-trip. Powers of two are where both went wrong, since the double
//! below one is twice as close as the double above (vendor/QUICKJS.md).

use ljos_engine::number_to_string;
use ljos_js::rquickjs::{Context, Function, Runtime};

/// How many significant digits `text` has.
fn digit_count(text: &str) -> usize {
    let mantissa = text.split_once('e').map_or(text, |(m, _)| m);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    digits.trim_matches('0').len().max(1)
}

fn every_number_checked() -> Vec<f64> {
    let mut xs = Vec::new();
    for biased in 0..2047u64 {
        xs.push(f64::from_bits(biased << 52));
    }
    for k in -1074..-1022 {
        xs.push(2f64.powi(k));
    }
    for k in -323..=308 {
        xs.push(format!("1e{k}").parse().unwrap());
    }
    let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..20_000 {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let x = f64::from_bits(s);
        if x.is_finite() {
            xs.push(x);
        }
    }
    let negated: Vec<f64> = xs.iter().map(|x| -x).collect();
    xs.extend(negated);
    xs.retain(|x| *x != 0.0);
    xs
}

#[test]
fn both_tiers_write_the_shortest_digits_of_every_power_of_two_and_ten() {
    let rt = Runtime::new().unwrap();
    let ctx = Context::full(&rt).unwrap();
    ctx.with(|ctx| {
        let string: Function = ctx.eval("(x) => String(x)").unwrap();
        let mut wrong = Vec::new();
        for x in every_number_checked() {
            let tier2: String = string.call((x,)).unwrap();
            let tier1 = number_to_string(x);
            let shortest = digit_count(&format!("{x:e}"));
            if tier1 != tier2 || tier1.parse::<f64>() != Ok(x) || digit_count(&tier1) != shortest {
                wrong.push(format!("{x:e}: tier 1 {tier1}, tier 2 {tier2}"));
            }
        }
        assert!(
            wrong.is_empty(),
            "{} wrong:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    });
}

#[test]
fn two_to_the_378_is_sixteen_digits() {
    let rt = Runtime::new().unwrap();
    let ctx = Context::full(&rt).unwrap();
    ctx.with(|ctx| {
        for source in [
            "String(2 ** 378)",
            "`${Math.pow(2, 378)}`",
            "(2 ** 378) + ''",
        ] {
            let text: String = ctx.eval(source).unwrap();
            assert_eq!(text, "6.156563468186638e+113", "{source}");
        }
        let text: String = ctx.eval("(2 ** 378).toString(10)").unwrap();
        assert_eq!(text, "6.156563468186638e+113");
    });
}
