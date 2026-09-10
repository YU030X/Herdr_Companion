# Herdr Companion Native Prototype

This is the A1 Slint prototype for the `<100 MiB` process-group Private Memory requirement. It is an independent executable and does not change the production Tauri app.

The prototype reuses the production Herdr transport, wire schema and snapshot normalization with `#[path]` imports. It implements a small read-only HUD with connection status, Workspace filtering, five-state ordering, task fallback text, stale snapshots and retry.

Run it from the repository root:

```powershell
cargo run --manifest-path native-prototype/Cargo.toml
```

Build an optimized binary for measurement:

```powershell
cargo build --manifest-path native-prototype/Cargo.toml --release
```

Measure the exact prototype process tree after starting the Release binary:

```powershell
.\scripts\Measure-NativePrototypeMemory.ps1 -Scenario ColdIdle
.\scripts\Measure-NativePrototypeMemory.ps1 -Scenario SixAgents
```

The script reports the process group Private Memory and Working Set. It does not require WebView2 descendants, so the result can be compared directly with the production measurement script's `ApplicationGroup` values.

The UI is intentionally a disposable measurement target. If it passes the memory, latency and visual gates, the next change should extract the shared Herdr code into a tracked core crate before production migration. If it fails, the prototype should be discarded without changing the Tauri release.
