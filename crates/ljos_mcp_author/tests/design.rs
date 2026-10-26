//! The design tools over the protocol, the way an agent calls them: what
//! they answer, that every number agrees with `ljos_design`, and that what
//! they suggest passes the checks they come from.

use std::sync::Arc;

use ljos_design::{color::Srgb, color::wcag_contrast, limits};
use ljos_mcp_author::{AuthorServer, Project, StubFrontEnd};
use ljos_mcp_core::{CapSet, DEFAULT_RESULT_BYTES, glue::RmcpServer};
use rmcp::{RoleClient, ServiceExt, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};

type Client = RunningService<RoleClient, ()>;

const ONE_COPY: usize = DEFAULT_RESULT_BYTES / 2;

async fn connect(dir: &tempfile::TempDir) -> Client {
    let server = AuthorServer::new(Project::open(dir.path()).unwrap(), Arc::new(StubFrontEnd));
    let (client_io, server_io) = tokio::io::duplex(256 * 1024);
    tokio::spawn(async move {
        let _ = RmcpServer::new(Arc::new(server), CapSet::EMPTY)
            .serve(server_io)
            .await;
    });
    ().serve(client_io).await.expect("handshake")
}

async fn call(c: &Client, name: &str, args: Value) -> (bool, Value) {
    let args = args.as_object().cloned().unwrap_or_default();
    let res = c
        .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(args))
        .await
        .unwrap();
    let body = res.structured_content.expect("structured content");
    let size = serde_json::to_vec(&body).unwrap().len();
    assert!(size <= ONE_COPY, "`{name}` answered {size} bytes");
    assert!(body.get("truncated").is_none(), "`{name}` was cut: {body}");
    (res.is_error == Some(true), body)
}

async fn ok(c: &Client, name: &str, args: Value) -> Value {
    let (is_error, body) = call(c, name, args).await;
    assert!(!is_error, "`{name}` was refused: {body}");
    body
}

async fn refused(c: &Client, name: &str, args: Value) -> String {
    let (is_error, body) = call(c, name, args).await;
    assert!(is_error, "`{name}` answered: {body}");
    body["error"]["code"].as_str().unwrap().to_owned()
}

/// Every item of `list` over all pages.
async fn all_pages(c: &Client, name: &str, args: Value, list: &str) -> Vec<Value> {
    let mut items = Vec::new();
    let mut args = args;
    loop {
        let body = ok(c, name, args.clone()).await;
        items.extend(body[list].as_array().unwrap().iter().cloned());
        match body["next_cursor"].as_str() {
            Some(next) => args["cursor"] = json!(next),
            None => return items,
        }
        assert!(items.len() < 10_000);
    }
}

#[tokio::test]
async fn every_rule_comes_back_once_with_its_guide() {
    let dir = tempfile::tempdir().unwrap();
    let c = connect(&dir).await;
    let rules = all_pages(&c, "design_rules", json!({}), "rules").await;
    let codes: Vec<&str> = rules.iter().map(|r| r["code"].as_str().unwrap()).collect();
    let want: Vec<&str> = ljos_design::RULES.iter().map(|r| r.code).collect();
    assert_eq!(codes, want);

    let one = ok(
        &c,
        "design_rules",
        json!({ "code": "motion-ease-in-enter" }),
    )
    .await;
    assert_eq!(one["rules"].as_array().unwrap().len(), 1);
    assert_eq!(one["rules"][0]["area"], "motion");
    assert_eq!(
        one["rules"][0]["guide"],
        "docs/design/motion.md#motion-ease-in-enter"
    );
    let text = one["limits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "text_contrast")
        .unwrap();
    assert_eq!(text["value"], json!(limits::TEXT_CONTRAST));

    let color = all_pages(&c, "design_rules", json!({ "area": "color" }), "rules").await;
    assert!(color.iter().all(|r| r["area"] == "color") && !color.is_empty());
    assert_eq!(
        refused(&c, "design_rules", json!({ "code": "nope" })).await,
        "not_found"
    );
}

#[tokio::test]
async fn motion_tokens_are_within_limits_and_never_ease_in() {
    let dir = tempfile::tempdir().unwrap();
    let c = connect(&dir).await;
    let tokens = all_pages(&c, "design_motion", json!({}), "tokens").await;
    assert_eq!(tokens.len(), ljos_design::motion::tokens().len());
    let curves = ok(&c, "design_motion", json!({ "kind": "enter" })).await["curves"].clone();
    for t in &tokens {
        let ms = t["duration_ms"].as_u64().unwrap() as u32;
        if t["kind"] != "loop" {
            assert!(ms <= t["limit_ms"].as_u64().unwrap() as u32, "{t}");
            assert!(ms >= limits::motion::MIN_VISIBLE_MS, "{t}");
        }
        let curve = curves
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == t["easing"])
            .unwrap();
        // The points are the library's own, and they say what the curve does.
        let b = ljos_design::motion::CURVES
            .into_iter()
            .find(|b| b.name == t["easing"])
            .unwrap();
        let points: Vec<f64> = curve["bezier"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        for (got, want) in points.iter().zip([b.x1, b.y1, b.x2, b.y2]) {
            assert!((got - f64::from(want)).abs() < 1e-4, "{t}");
        }
        assert!(!b.overshoots(), "{t}");
        if t["kind"] != "loop" && t["kind"] != "move" {
            // An In* curve starts slow; nothing that comes or goes may.
            // Something already in view may ease in and out as it moves.
            assert!(!b.starts_slow(), "{t}");
            assert!(
                t["qml"].as_str().unwrap().contains("Easing.BezierSpline")
                    || t["spring"].is_object()
            );
        }
        assert!(
            t["reduced"]["duration_ms"].as_u64().unwrap() as u32 <= limits::motion::MICRO_MAX_MS
        );
        assert_eq!(t["reduced"]["property"], "opacity");
        if let Some(ratio) = t["spring"]["damping_ratio"].as_f64() {
            assert!(
                ratio as f32 >= limits::motion::SPRING_MIN_DAMPING_RATIO,
                "{t}"
            );
        }
    }

    let enter = ok(
        &c,
        "design_motion",
        json!({ "kind": "enter", "size": "panel", "frequency": "occasional", "properties": "opacity" }),
    )
    .await;
    assert_eq!(
        enter["tokens"][0]["qml"],
        "NumberAnimation { properties: \"opacity\"; duration: 250; easing.type: Easing.BezierSpline; easing.bezierCurve: [0.23, 1, 0.32, 1, 1, 1] }"
    );
    let exit = ok(
        &c,
        "design_motion",
        json!({ "kind": "exit", "size": "panel", "frequency": "occasional" }),
    )
    .await;
    assert!(exit["tokens"][0]["duration_ms"].as_u64() < enter["tokens"][0]["duration_ms"].as_u64());
    let gesture = ok(
        &c,
        "design_motion",
        json!({ "kind": "gesture", "size": "small", "frequency": "occasional" }),
    )
    .await;
    assert!(
        gesture["tokens"][0]["qml"]
            .as_str()
            .unwrap()
            .starts_with("SpringAnimation {")
    );
    assert_eq!(
        refused(&c, "design_motion", json!({ "kind": "wobble" })).await,
        "invalid_argument"
    );
}

#[tokio::test]
async fn a_palette_passes_every_pair_it_lists() {
    let dir = tempfile::tempdir().unwrap();
    let c = connect(&dir).await;
    for (seed, scheme, contrast) in [
        ("#1d4ed8", "light", "normal"),
        ("250", "dark", "normal"),
        ("gold", "light", "high"),
        ("#00ff00", "dark", "high"),
    ] {
        let p = ok(
            &c,
            "design_palette",
            json!({ "seed": seed, "scheme": scheme, "contrast": contrast }),
        )
        .await;
        assert_eq!(p["failing"], 0, "{seed} {scheme} {contrast}");
        let role = |name: &str| {
            let hex = p["roles"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["name"] == name)
                .unwrap()["color"]
                .as_str()
                .unwrap()
                .to_owned();
            Srgb::parse(&hex).unwrap()
        };
        let want = if contrast == "high" {
            7.0
        } else {
            limits::TEXT_CONTRAST
        };
        assert!(wcag_contrast(role("text"), role("surface")) >= want);
        assert!(wcag_contrast(role("onAccent"), role("accent")) >= want);
        assert!(wcag_contrast(role("focusRing"), role("canvas")) >= limits::NONTEXT_CONTRAST);
        let checks = p["checks"].as_array().unwrap();
        assert!(checks.len() > 40);
        assert!(
            checks
                .iter()
                .all(|c| c["ratio"].as_f64() >= c["required"].as_f64())
        );
        assert_eq!(p["ramps"].as_array().unwrap().len(), 6);
        assert_eq!(p["ramps"][0]["steps"].as_array().unwrap().len(), 12);
        assert_eq!(p["harmonies"].as_array().unwrap().len(), 4);
    }
    assert_eq!(
        refused(&c, "design_palette", json!({ "seed": "nope" })).await,
        "invalid_argument"
    );
    assert_eq!(
        refused(
            &c,
            "design_palette",
            json!({ "seed": "250", "scheme": "sepia" })
        )
        .await,
        "invalid_argument"
    );
}

#[tokio::test]
async fn contrast_blends_layers_and_offers_the_nearest_pass() {
    let dir = tempfile::tempdir().unwrap();
    let c = connect(&dir).await;
    let plain = ok(
        &c,
        "design_contrast",
        json!({ "foreground": "#000000", "background": "#ffffff" }),
    )
    .await;
    assert_eq!(plain["ratio"], json!(21.0));
    assert_eq!(plain["passes"], true);
    assert!(plain["nearest_passing"].is_null());

    // Grey text on a half-black card over white: the card is mid grey.
    let layered = ok(
        &c,
        "design_contrast",
        json!({
            "foreground": "#999999",
            "background": "#ffffff",
            "layers": [{ "color": "#000000", "opacity": 0.5 }],
        }),
    )
    .await;
    assert_eq!(layered["background"], "#808080");
    assert_eq!(layered["passes"], false);
    assert_eq!(layered["rule"], "color-text-contrast");
    let fixed = Srgb::parse(layered["nearest_passing"].as_str().unwrap()).unwrap();
    assert!(wcag_contrast(fixed, Srgb::parse("#808080").unwrap()) >= limits::TEXT_CONTRAST);

    // Half-transparent text is what it blends to.
    let faded = ok(
        &c,
        "design_contrast",
        json!({ "foreground": "#000000", "background": "#ffffff", "opacity": 0.5 }),
    )
    .await;
    assert_eq!(faded["foreground"], "#808080");

    let large = ok(
        &c,
        "design_contrast",
        json!({ "foreground": "#888888", "background": "#ffffff", "pixel_size": 24 }),
    )
    .await;
    assert_eq!(large["required"], json!(limits::LARGE_TEXT_CONTRAST));
    let mark = ok(
        &c,
        "design_contrast",
        json!({ "foreground": "#888888", "background": "#ffffff", "kind": "nontext" }),
    )
    .await;
    assert_eq!(mark["rule"], "color-nontext-contrast");
    assert_eq!(
        refused(
            &c,
            "design_contrast",
            json!({ "foreground": "#000", "background": "#80ffffff" })
        )
        .await,
        "invalid_argument"
    );
}
