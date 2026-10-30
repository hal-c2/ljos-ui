//! The agent's side of ARCHITECTURE.md §19.3 and the author's command line
//! (features/tooling). There is no application: the files of the scenario
//! are a project in a directory of its own, `ljos mcp` is started on it and
//! `ljos` is run in it. The agent steps of `inspector` speak to it once it
//! is connected. The attributes stay on one line each.

use cucumber::{given, then, when};
use gherkin::Step;

use crate::value;
use crate::world::LjosWorld;

use super::inspector::unquoted;
use super::{StepResult, docstring};

//
// The agent's side of ARCHITECTURE.md §19.3 and the author's command line
// (features/tooling). There is no application: the files of the scenario
// are a project in a directory of its own, `ljos mcp` is started on it and
// `ljos` is run in it. The agent steps above speak to it once it is
// connected. The attributes stay on one line each.

/// A doc string as the text it stands for: no label, no indent.
fn author_doc(step: &Step) -> Result<String, String> {
    Ok(crate::author::text_of(&docstring(step)?))
}

#[given("the authoring tools are connected")]
fn author_connected(w: &mut LjosWorld) -> StepResult {
    let (author, files) = w.author();
    author.connect(files, "", None)
}

#[given(regex = r#"^the authoring tools are connected with the capabilities "([^"]+)"$"#)]
fn author_connected_with(w: &mut LjosWorld, caps: String) -> StepResult {
    let (author, files) = w.author();
    author.connect(files, &caps, None)
}

#[rustfmt::skip]
#[given(regex = r#"^the authoring tools are connected with the capabilities "([^"]+)" and the features runner "([^"]+)"$"#)]
fn author_connected_with_runner(w: &mut LjosWorld, caps: String, runner: String) -> StepResult {
    let (author, files) = w.author();
    author.connect(files, &caps, Some(&runner))
}

#[given(regex = r#"^the file "([^"]+)" outside the project:$"#)]
fn author_outside_file(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let text = docstring(step)?;
    let (author, files) = w.author();
    author.outside_file(files, &name, &text)
}

#[given(regex = r#"^the project's "([^"]+)" is a link to "([^"]+)" outside the project$"#)]
fn author_link_out(w: &mut LjosWorld, link: String, target: String) -> StepResult {
    let (author, files) = w.author();
    author.link_out(files, &link, &target)
}

#[when("the agent lists the resources")]
fn agent_lists_resources(w: &mut LjosWorld) -> StepResult {
    let (author, files) = w.author();
    author.resources(files)
}

#[when(regex = r#"^the agent asks for the prompt "([^"]+)"$"#)]
fn agent_asks_prompt(w: &mut LjosWorld, name: String) -> StepResult {
    let (author, files) = w.author();
    author.prompt(files, &name, &serde_json::json!({}))
}

#[when(regex = r#"^the agent asks for the prompt "([^"]+)" with:$"#)]
fn agent_asks_prompt_with(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let args = serde_json::from_str(&author_doc(step)?)
        .map_err(|e| format!("the arguments are not JSON: {e}"))?;
    let (author, files) = w.author();
    author.prompt(files, &name, &args)
}

#[given(regex = r#"^the author's desktop setting "([^"]+)" "([^"]+)" is (.+)$"#)]
fn author_desktop_setting(
    w: &mut LjosWorld,
    namespace: String,
    key: String,
    value: String,
) -> StepResult {
    let value = match value.as_str() {
        "true" => crate::author::Setting::Bool(true),
        "false" => crate::author::Setting::Bool(false),
        v => match v.strip_prefix("uint32 ") {
            Some(n) => crate::author::Setting::Uint(
                n.parse().map_err(|_| format!("{n:?} is not a uint32"))?,
            ),
            None => crate::author::Setting::Text(
                v.strip_prefix('"')
                    .and_then(|v| v.strip_suffix('"'))
                    .ok_or_else(|| format!("{v:?} is not \"text\", uint32 N, true or false"))?
                    .to_owned(),
            ),
        },
    };
    let (author, files) = w.author();
    author.desktop_setting(files, &namespace, &key, value)
}

#[given(regex = r#"^the author has run "([^"]+)"$"#)]
#[when(regex = r#"^the author runs "([^"]+)"$"#)]
fn author_runs(w: &mut LjosWorld, line: String) -> StepResult {
    let (author, files) = w.author();
    author.run(files, &line)
}

// The application is left running, and the scenario's agent speaks to
// the inspector it names on its errors, as a parent process finds it.
#[when(regex = r#"^the author starts "([^"]+)"$"#)]
fn author_starts(w: &mut LjosWorld, line: String) -> StepResult {
    let (author, files) = w.author();
    author.start(files, &line)
}

// The inspector's rate (ARCHITECTURE.md §19.7), counted by a client that
// speaks plain HTTP so that a refusal is not asked again.
#[rustfmt::skip]
#[when(regex = r"^a client makes (\d+) calls to the started application's inspector within one second$")]
fn client_makes_calls(w: &mut LjosWorld, count: usize) -> StepResult {
    w.author().0.call_inspector(count)
}

#[then(regex = r"^the first (\d+) calls are answered$")]
fn calls_answered(w: &mut LjosWorld, first: usize) -> StepResult {
    w.author().0.calls_answered(first)
}

#[then(regex = r#"^call (\d+) is refused with status (\d+) and "([^"]+)"$"#)]
fn call_refused(w: &mut LjosWorld, nth: usize, status: u16, header: String) -> StepResult {
    w.author().0.call_refused(nth, status, &header)
}

#[then(regex = r"^the started application's token file says (\d+) calls a second$")]
fn token_file_rate(w: &mut LjosWorld, rate: u64) -> StepResult {
    w.author().0.token_file_rate(rate)
}

// Who may get at the token (ARCHITECTURE.md §19.7): the mode is read from
// the system, as somebody else trying the directory would meet it.
#[given(regex = r#"^the project's directory "([^"]+)" was made with the mode (\d+)$"#)]
fn project_dir_made_with_mode(w: &mut LjosWorld, dir: String, mode: String) -> StepResult {
    let (author, files) = w.author();
    author.make_dir(files, &dir, &mode)
}

#[then(regex = r#"^the project's directory "([^"]+)" has the mode (\d+)$"#)]
fn project_dir_has_mode(w: &mut LjosWorld, dir: String, mode: String) -> StepResult {
    let (author, files) = w.author();
    author.dir_has_mode(files, &dir, &mode)
}

#[given(regex = r#"^the project's directory "([^"]+)" is a link to "([^"]+)"$"#)]
fn project_dir_is_link(w: &mut LjosWorld, dir: String, to: String) -> StepResult {
    let (author, files) = w.author();
    author.make_dir_link(files, &dir, &to)
}

#[then(regex = r#"^the project's directory "([^"]+)" holds no file$"#)]
fn project_dir_holds_no_file(w: &mut LjosWorld, dir: String) -> StepResult {
    let (author, files) = w.author();
    author.dir_holds_no_file(files, &dir)
}

#[then(regex = r"^the started application's token file has the mode (\d+)$")]
fn token_file_mode(w: &mut LjosWorld, mode: String) -> StepResult {
    w.author().0.token_file_has_mode(&mode)
}

#[then(regex = r#"^the application's errors mention "(.*)"$"#)]
fn application_errors_mention(w: &mut LjosWorld, text: String) -> StepResult {
    w.author().0.errors_mention(&unquoted(&text))
}

#[then(regex = r#"^the prompt contains "(.*)"$"#)]
fn prompt_contains(w: &mut LjosWorld, text: String) -> StepResult {
    let text = unquoted(&text);
    let prompt = &w.author().0.seen()?.answered()?.text;
    if prompt.contains(&text) {
        Ok(())
    } else {
        Err(format!("{text:?} is not in the prompt:\n{prompt}"))
    }
}

#[then(regex = r#"^the prompt names the tools "([^"]+)"$"#)]
fn prompt_names(w: &mut LjosWorld, names: String) -> StepResult {
    let names: Vec<&str> = names.split(',').map(str::trim).collect();
    w.author().0.prompt_names(&names)
}

#[then(regex = r#"^the agent is offered the prompts "([^"]+)"$"#)]
fn agent_offered_prompts(w: &mut LjosWorld, names: String) -> StepResult {
    let names: Vec<&str> = names.split(',').map(str::trim).collect();
    let (author, files) = w.author();
    let offered = author.prompts(files)?;
    if offered == names {
        Ok(())
    } else {
        Err(format!("the agent is offered the prompts {offered:?}"))
    }
}

#[then(regex = r#"^the authoring tools' instructions match "(.*)"$"#)]
fn author_instructions_match(w: &mut LjosWorld, re: String) -> StepResult {
    w.author().0.seen()?.instructions_match(&unquoted(&re))
}

#[then(regex = r#"^the answer's "(/[^"]*)" is:$"#)]
fn answer_text_is(w: &mut LjosWorld, pointer: String, step: &Step) -> StepResult {
    let want = author_doc(step)?;
    w.author().0.answer_text_is(&pointer, &want)
}

#[then(regex = r#"^the file "([^"]+)" is:$"#)]
fn file_is(w: &mut LjosWorld, name: String, step: &Step) -> StepResult {
    let want = author_doc(step)?;
    let (author, files) = w.author();
    author.file_is(files, &name, &want)
}

// The directory stands for `/`: `proc/meminfo`, `proc/self/cgroup` and
// `sys/fs/cgroup/…/memory.max` are read under it.
#[then(
    regex = r#"^a (desktop|phone) with the system files under "([^"]+)" keeps up to ([\d.]+) MiB of decoded pictures$"#
)]
fn image_budget(w: &mut LjosWorld, class: String, root: String, mib: f64) -> StepResult {
    use ljos_runtime::{DeviceClass, image_cache_budget, usable_memory_under};
    let (author, files) = w.author();
    let root = author.project_path(files, &root)?;
    let class = if class == "phone" {
        DeviceClass::Phone
    } else {
        DeviceClass::Desktop
    };
    let bytes = image_cache_budget(class, usable_memory_under(&root));
    #[allow(clippy::cast_precision_loss)]
    let actual = bytes as f64 / (1024.0 * 1024.0);
    // To the hundredth of a MiB the scenario names it by.
    if (actual - mib).abs() < 0.005 {
        Ok(())
    } else {
        Err(format!("the budget is {actual:.2} MiB ({bytes} bytes)"))
    }
}

// The value is JSON, as in `the answer's "/pointer" is V`.
#[then(regex = r#"^the JSON file "([^"]+)" has "(/[^"]*)" equal to (.+)$"#)]
fn json_file_has(w: &mut LjosWorld, name: String, pointer: String, json: String) -> StepResult {
    let want = value::parse(&json)?;
    let (author, files) = w.author();
    author.json_file_has(files, &name, &pointer, &want)
}

#[then(regex = r#"^the picture "([^"]+\.png)" is (\d+) by (\d+) pixels$"#)]
fn picture_file_is(w: &mut LjosWorld, name: String, width: u32, height: u32) -> StepResult {
    let (author, files) = w.author();
    author.picture_is(files, &name, width, height)
}

#[then("the command prints:")]
fn command_prints(w: &mut LjosWorld, step: &Step) -> StepResult {
    let want = author_doc(step)?;
    w.author().0.ran()?.printed(&want)
}

#[then(regex = r"^the command exits with status (\d+)$")]
fn command_exits(w: &mut LjosWorld, status: i32) -> StepResult {
    w.author().0.ran()?.exited(status)
}

#[then(regex = r#"^the command's output mentions "(.*)"$"#)]
fn command_mentions(w: &mut LjosWorld, text: String) -> StepResult {
    w.author().0.ran()?.mentions(&unquoted(&text), true)
}

#[then(regex = r#"^the command's output does not mention "(.*)"$"#)]
fn command_mentions_not(w: &mut LjosWorld, text: String) -> StepResult {
    w.author().0.ran()?.mentions(&unquoted(&text), false)
}
