# Architecture

## Processes and threads

- **Main thread**: GPUI. All views and app state live here as `Entity<T>`s.
- **`t3-net` tokio runtime** (`t3_client::runtime`): every socket and HTTP request. Work crosses
  over with `t3_client::runtime::spawn(fut)`, whose returned future can be awaited from GPUI's
  executor. tokio channels (`mpsc`, `oneshot`, `watch`) are executor-agnostic, so GPUI tasks
  `recv().await` on them directly.

No blocking IO on the main thread. Disk reads at startup are allowed only for small JSON files.

## Data flow

```
server ──ws──▶ RpcConnection ──typed items──▶ sync task: reducer (pure) ──Arc<State>──▶ watch
               (t3-client, t3-net runtime)                                                │
                         GPUI task (cx.spawn): rx.changed().await, borrow the Arc ◀───────┘
                                 │
                       Entity<EnvironmentState>.update(..) + cx.notify()
                                 │
                         views re-render what changed (rows are Arcs: compare with ptr_eq)
```

- `t3-protocol` holds serde types for upstream contracts. Unknown fields are ignored; string
  unions keep an `Other(String)` variant and tagged unions an `Unknown` variant, so a newer server
  never breaks decoding.
- `t3-client` holds the reducers as plain functions over protocol types
  (`ShellState::apply(&mut self, item)`, `ThreadState::apply(...)`). They have no GPUI types, so
  they are tested with recorded server frames (`crates/t3-client/tests/fixtures`).
- `t3-client::Environment` runs the reducers on the networking runtime and publishes
  `Arc<ShellState>` / `Arc<ThreadState>` on `tokio::sync::watch` channels. A slow UI only ever
  sees the latest state; it never has to replay every delta. Rows inside are `Arc`s shared with
  the previous state, so only changed rows are new allocations.
- `t3-app` owns entities that hold the latest published state and the GPUI tasks that feed them.

## App state (t3-app)

```
AppState (global Entity)
├── environments: ordered map EnvironmentId → Entity<Environment>
│     ├── descriptor (label, kind, url, credential ref)
│     ├── status: Connecting | Connected | Reconnecting{attempt} | Blocked{reason} | Offline
│     ├── config: Option<ServerConfig>
│     ├── shell: ShellState            (projects + thread summaries, from subscribeShell)
│     └── threads: map ThreadId → Entity<ThreadState>   (subscribeThread while open)
├── route: Route                        (which main view is showing)
├── ui: UiState                          (sidebar width/collapsed, panels, theme) — persisted
├── drafts: Entity<DraftStore>           (composer drafts) — persisted
└── cloud: Entity<CloudAccount>          (T3 Connect session + linked environments)
```

`Route` mirrors the web router: `Index`, `Thread { environment, thread }`, `Draft { id }`,
`Settings(SettingsPage)`, `Pair { .. }`.

## Views (t3-app)

```
Workspace (root)
├── Sidebar
├── main column: ChatView | SettingsView | IndexView
│     ChatView: ChatHeader, Timeline (gpui `list`, bottom-anchored), Composer, BranchToolbar
├── RightPanel (diff | plan | files)
└── TerminalDrawer
```

Each view is an `Entity<T>` that reads `AppState`/`Environment` entities in `render` and subscribes
to the ones it shows. Stateless pieces (rows, chips, buttons) are `RenderOnce` components in
`t3-ui` or a local `components` module.

## Persistence (macOS)

`~/Library/Application Support/T3UI/`:
- `environments.json`: saved environments (no secrets)
- `ui-state.json`: layout, theme, last route
- `drafts.json`: composer drafts
- secrets (bearer/session tokens): see `docs/spec/connections.md` for the chosen store.

## Verification

- `t3-snapshots` renders scenes of the real views headlessly (macOS Metal) to PNG. Scenes feed
  recorded server data (fixtures) into the same entities the live app uses.
- Live end-to-end runs (CI, macOS): `npx t3@nightly` with a fake Codex provider, seeded by
  `e2e/seed.mjs`, then the real app connects to it and captures screenshots of scripted steps.
- Every CI run uploads the DMG and the PNGs as artifacts.
