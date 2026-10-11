# The example suite

A suite small enough to read, for proving the mechanism and as the model for a real one. Its
"repository" is `fixture/`, inside this repository, so its `PIN` is a digest instead of a commit.

The features are about a made-up kiosk and use their own words (`the visitor taps Add`,
`a welcome screen for the badge "Ada"`), not Ljós's step catalogue. `steps/src/main.rs` is the adapter:
it writes the kiosk's screens as QML (a real suite would load the application's own QML) and maps each
sentence onto `Backend`.

| File | Shows |
|---|---|
| `fixture/features/counter.feature` | plain scenarios, held |
| `fixture/features/greeting.feature` | an outline: each example row has its own id |
| `fixture/features/later.feature` | `@backlog-ljos` (skipped and counted) and a scenario that fails today and is not held |

`steps/tests/suite.rs` runs the adapter on copies of the suite: the committed ratchet passes and the
fixture is left unchanged, a regression fails, `--raise` adds and never lowers, a wrong commit or digest
is refused, a glob that matches nothing is an error. Run them with `mise run features:acceptance-test`.

After changing a file under `fixture/`: `mise run features:acceptance example --digest`, put the result
in `PIN`, then `mise run features:acceptance example --raise`.
