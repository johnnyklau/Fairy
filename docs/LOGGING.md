# Logging

One build, not two. Fairy always logs — the difference between "normal"
and "debugging" is a `RUST_LOG` environment variable at launch time, not a
separate binary to build and keep in sync.

## Where the log lives

`<app data dir>/logs/fairy.log.<date>` — same folder as `settings.json`
and the voice model, so it's wherever the rest of Fairy's own data
already is (e.g. `%APPDATA%\com.fairy.app\logs\` on Windows). A new file
starts each day (`tracing_appender::rolling::daily`); old ones aren't
auto-deleted, so clean them up by hand if they ever pile up enough to
matter.

## Verbosity

By default Fairy logs at `info` and above — reminders firing, synthesis
outcomes, model download/ready state, update checks — enough to
reconstruct what happened after the fact without asking anyone to
reproduce anything.

For more detail (every scheduler tick, cache hits, per-file model
verification), launch with `RUST_LOG` set:

```bash
$env:RUST_LOG = "fairy_lib=debug"   # PowerShell
./fairy.exe
```

```bash
RUST_LOG=fairy_lib=debug ./fairy.exe   # bash
```

`fairy_lib=trace` goes further still (every tick, whether it fired or
not). Plain `RUST_LOG=debug` also works but includes Tauri's and every
dependency's own debug output too, which is usually more noise than
signal — scoping to `fairy_lib=` keeps it to Fairy's own code.

## What's instrumented

- `behavior.rs` — every scheduler tick's decision (or lack of one), and
  the full outcome of showing a reminder (voice enabled, text, whether
  audio attached, its duration, the timestamp it was triggered at).
- `voice.rs` — cache hit/miss on every synthesis call, inference timing,
  model-ready checks (including a `warn` whenever the model manifest is
  missing or a file's size doesn't match it — see the corrupted-model
  entry in Field Notes for why that check exists), download progress.
- `updater.rs` — check start/result, install start/result.
- `shell.rs` — autostart failures.

## Why not a separate debug build

The actual cost this was solving wasn't "no logging exists" — it was
temporary `eprintln!`s getting hand-added and removed for every
debugging session. A second build configuration doesn't fix that; it
just adds a second thing to keep in sync (which binary ships, whether
RELEASING.md's steps apply to both). One build with runtime-controlled
verbosity avoids both problems at once.
