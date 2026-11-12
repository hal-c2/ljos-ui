//! Tests of the network: `XMLHttpRequest` written as `fetch`.

use super::*;

/// A file whose `load` makes the request `request`, one statement a line.
fn loading(request: &str) -> String {
    format!(
        "import QtQuick\nItem {{\n    property string got: \"\"\n    function load() {{\n{request}    }}\n}}\n"
    )
}

#[test]
fn the_usual_shape_is_one_fetch_and_every_line_stays() {
    let source = loading(
        "        var http = new XMLHttpRequest()\n        http.open(\"GET\", \"https://example.org/a\", true)\n        http.onreadystatechange = function() {\n            if (http.readyState === 4) {\n                got = http.status + \" \" + http.responseText\n            }\n        }\n        http.send()\n",
    );
    assert_eq!(
        lowered(&source),
        "pragma Scripts: JavaScript; import Ljos.Ui\nItem {\n    property string got: \"\"\n    function load() {\n\n\n        fetch(\"https://example.org/a\").catch(() => Response.error()).then(async (http: Response) => {\n\n            got = http.status + \" \" + (await http.text())\n\n        })\n\n    }\n}\n"
    );
}

#[test]
fn what_is_sent_is_the_fetch_s_init() {
    let source = loading(
        "        const r = new XMLHttpRequest()\n        r.open(\"POST\", url)\n        r.setRequestHeader(\"A\", \"1\")\n        r.setRequestHeader(\"B\", b)\n        r.responseType = \"json\"\n        r.onreadystatechange = () => {\n            if (r.readyState !== XMLHttpRequest.DONE) return\n            got = r.response.name + r.responseURL + r.getResponseHeader(\"x\")\n        }\n        r.send(body)\n",
    );
    let lowered = lowered(&source);
    assert!(
        lowered.contains("fetch(url, { method: \"POST\", headers: { \"A\": \"1\", \"B\": b }, body: body }).catch(() => Response.error()).then(async (r: Response) => {\n"),
        "{lowered}"
    );
    assert!(
        lowered.contains(
            "got = (await r.json().catch(() => null)).name + r.url + r.headers.get(\"x\")\n"
        ),
        "{lowered}"
    );
    assert_eq!(lowered.lines().count(), source.lines().count());
}

#[test]
fn a_json_body_read_twice_is_null_where_it_is_not_json_and_the_status_check_still_runs() {
    let source = loading(
        "        const r = new XMLHttpRequest()\n        r.open(\"GET\", \"u\")\n        r.responseType = \"json\"\n        r.onreadystatechange = function() {\n            if (r.readyState !== 4) return\n            if (r.status === 200) { got = r.response.a + r.response.b } else { console.log(\"failed\") }\n        }\n        r.send()\n",
    );
    let lowered = lowered(&source);
    assert!(
        lowered.contains(
            "then(async (r: Response) => { const rJson = await r.json().catch(() => null);\n"
        ),
        "{lowered}"
    );
    assert!(
        lowered.contains(
            "if (r.status === 200) { got = rJson.a + rJson.b } else { console.log(\"failed\") }"
        ),
        "{lowered}"
    );
    assert_eq!(lowered.lines().count(), source.lines().count());
}

#[test]
fn a_header_named_twice_is_a_list_of_pairs() {
    let source = loading(
        "        const r = new XMLHttpRequest()\n        r.open(\"GET\", \"u\")\n        r.setRequestHeader(\"A\", \"1\")\n        r.setRequestHeader(\"a\", \"2\")\n        r.onreadystatechange = function() { if (r.readyState == 4) got = r.responseText }\n        r.send(null)\n",
    );
    assert!(
        lowered(&source).contains(
            "fetch(\"u\", { headers: [[\"A\", \"1\"], [\"a\", \"2\"]] }).catch(() => Response.error()).then(async (r: Response) => { got = (await r.text()) })"
        ),
        "{}",
        lowered(&source)
    );
}

#[test]
fn a_body_read_twice_is_read_once_at_the_top() {
    let source = loading(
        "        const r = new XMLHttpRequest()\n        r.open(\"GET\", \"u\")\n        r.onreadystatechange = function() {\n            if (r.readyState != 4) { return }\n            got = r.responseText + r.responseText\n        }\n        r.send()\n",
    );
    let lowered = lowered(&source);
    assert!(
        lowered.contains("then(async (r: Response) => { const rText = await r.text();\n"),
        "{lowered}"
    );
    assert!(lowered.contains("got = rText + rText\n"), "{lowered}");
}

#[test]
fn another_shape_keeps_the_file_and_says_why() {
    let request = |open: &str, read: &str| {
        loading(&format!(
            "        const http = new XMLHttpRequest()\n        {open}\n        http.onreadystatechange = function() {{\n            if (http.readyState === 4)\n                got = {read}\n        }}\n        http.send()\n"
        ))
    };
    for (source, why) in [
        (
            request("http.open(\"GET\", \"u\", false)", "http.responseText"),
            "the request waits for its reply",
        ),
        (
            request(
                "http.open(\"GET\", \"u\"); http.timeout = 5",
                "http.responseText",
            ),
            "the request's `timeout` has no `fetch`",
        ),
        (
            request("http.open(\"GET\", \"u\")", "http.responseXML"),
            "the request's `responseXML` has no `fetch`",
        ),
        (
            request("http.open(\"GET\", \"u\")", "String(http.readyState)"),
            "`readyState` is read where `fetch` has no such thing",
        ),
        (
            loading(
                "        const http = new XMLHttpRequest()\n        http.open(\"GET\", \"u\")\n        http.onreadystatechange = function() { if (http.readyState === 4) got = http.responseText }\n        http.send()\n        requests.push(http)\n",
            ),
            "the request is handed on",
        ),
        (
            request("http.open(\"GET\", \"u\")", "this.responseText"),
            "the handler reads `this`",
        ),
        (
            request("http.open(\"GET\", \"u\"); got = \"\"", "http.responseText"),
            "code runs between the request's `open` and its `send`",
        ),
        (
            loading(
                "        const http = new XMLHttpRequest()\n        http.open(\"GET\", \"u\")\n        http.send()\n",
            ),
            "is not opened, answered in `onreadystatechange` and sent",
        ),
        (
            loading(
                "        const http = new XMLHttpRequest()\n        http.open(\"GET\", \"u\")\n        http.onreadystatechange = function() { got = http.responseText }\n        http.send()\n",
            ),
            "runs as the request goes",
        ),
        (
            loading(
                "        const http = new XMLHttpRequest()\n        http.open(\"GET\", \"u\")\n        http.onreadystatechange = done\n        http.send()\n",
            ),
            "is not a function of its own",
        ),
        (
            loading("        requests.push(new XMLHttpRequest())\n"),
            "is made where it is not a local of its own",
        ),
        (
            loading(
                "        const http = new XMLHttpRequest()\n        http.open(\"POST\", \"u\")\n        http.onreadystatechange = function() { if (http.readyState === 4) got = http.responseText }\n        http.send(JSON.stringify({\n            name: got\n        }))\n",
            ),
            "spans lines",
        ),
    ] {
        let kept = kept_for(&source);
        assert!(kept.contains(why), "{kept}\n{source}");
    }
}

#[test]
fn a_class_of_the_script_s_own_is_not_the_request() {
    let source = loading(
        "        function XMLHttpRequest() { return {} }\n        const http = new XMLHttpRequest()\n",
    );
    assert!(lowered(&source).contains("new XMLHttpRequest()"));
}

#[test]
fn an_open_whose_arguments_sit_on_lines_of_their_own_is_gathered_and_keeps_its_lines() {
    let source = loading(
        "        const http = new XMLHttpRequest()\n        http.open(\"GET\",\n            \"u\")\n        http.onreadystatechange = function() { if (http.readyState === 4) got = http.responseText }\n        http.send()\n",
    );
    let text = lowered(&source);
    assert_eq!(text.lines().count(), source.lines().count(), "{text}");
    assert!(
        text.contains("fetch(\"u\").catch(() => Response.error()).then(async (http: Response) => { got = (await http.text()) })"),
        "{text}"
    );
}
