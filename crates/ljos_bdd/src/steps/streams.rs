//! Side streams (ARCHITECTURE.md §3.3): a thread feeds the UI thread items
//! in batches, and the UI thread hands each batch to a handler in one turn.

use cucumber::{given, then, when};
use ljos_runtime::DrainBudget;

use crate::world::LjosWorld;

use super::StepResult;

#[given(
    regex = r#"^a stream "([^"]+)" of (\d+) items that (keeps the newest per key|refuses the newest when full|evicts the oldest when full)$"#
)]
fn stream_made(w: &mut LjosWorld, name: String, capacity: usize, policy: String) -> StepResult {
    w.side_streams().make(&name, capacity, &policy)
}

#[when(regex = r#"^(\d+) items are sent to the stream "([^"]+)"$"#)]
fn items_sent(w: &mut LjosWorld, count: usize, name: String) -> StepResult {
    w.side_streams()
        .send(&name, (1..=count).map(|i| i.to_string()))
}

// Items are separated by ", "; an item "key=value" is coalesced by its key.
#[when(regex = r#"^the stream "([^"]+)" is sent "([^"]*)"$"#)]
fn stream_sent(w: &mut LjosWorld, name: String, items: String) -> StepResult {
    w.side_streams()
        .send(&name, items.split(", ").map(str::to_owned))
}

#[when(regex = r#"^the UI thread handles the stream "([^"]+)"$"#)]
fn ui_handles(w: &mut LjosWorld, name: String) -> StepResult {
    w.side_streams().handle(&name, DrainBudget::UNLIMITED)
}

#[when(regex = r#"^the UI thread handles at most (\d+) items of the stream "([^"]+)"$"#)]
fn ui_handles_at_most(w: &mut LjosWorld, most: usize, name: String) -> StepResult {
    w.side_streams().handle(&name, DrainBudget::count(most))
}

#[then(regex = r#"^the UI thread was woken (\d+) times?$"#)]
fn ui_woken(w: &mut LjosWorld, times: usize) -> StepResult {
    let woken = w.side_streams().woken();
    if woken == times {
        Ok(())
    } else {
        Err(format!(
            "the UI thread was woken {woken} times, not {times}"
        ))
    }
}

#[then(regex = r#"^the stream "([^"]+)" was handled in (\d+) turns?(?: of (\d+) items)?$"#)]
fn stream_turns(w: &mut LjosWorld, name: String, turns: usize, items: String) -> StepResult {
    // An optional group that did not match is handed over empty.
    let items: Option<usize> = match items.as_str() {
        "" => None,
        n => Some(n.parse().map_err(|e| format!("{n:?}: {e}"))?),
    };
    let (ran, handed) = w.side_streams().handled(&name)?;
    if ran != turns {
        return Err(format!(
            "the stream {name:?} was handled in {ran} turns, not {turns}"
        ));
    }
    match items {
        Some(items) if handed.len() != items => Err(format!(
            "the stream {name:?} handed over {} items, not {items}",
            handed.len()
        )),
        _ => Ok(()),
    }
}

#[then(regex = r#"^the stream "([^"]+)" handed over "([^"]*)"$"#)]
fn stream_handed(w: &mut LjosWorld, name: String, items: String) -> StepResult {
    let (_, handed) = w.side_streams().handled(&name)?;
    let handed = handed.join(", ");
    if handed == items {
        Ok(())
    } else {
        Err(format!(
            "the stream {name:?} handed over {handed:?}, not {items:?}"
        ))
    }
}

#[then(regex = r#"^the stream "([^"]+)" holds (\d+) items$"#)]
fn stream_holds(w: &mut LjosWorld, name: String, items: usize) -> StepResult {
    let holds = w.side_streams().holds(&name)?;
    if holds == items {
        Ok(())
    } else {
        Err(format!(
            "the stream {name:?} holds {holds} items, not {items}"
        ))
    }
}
