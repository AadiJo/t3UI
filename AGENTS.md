# AGENTS.md

T3UI is a native Rust/GPUI port of the T3 Code desktop client. The goal is a client that
is **visually and interactively identical** to the reference UI, but faster.

## Sources of truth

| What | Where | Notes |
| --- | --- | --- |
| Visuals + behavior | `~/L-Projects/t3UI-refs/t3code-fork` = github.com/AadiJo/t3code-again `main` @ `fe7d3092c` (`apps/web`, `apps/desktop`) | Read-only. Match it exactly. **Not** `~/L-Projects/t3code-again` (a stale July checkout, 4,601 commits behind). |
| Wire protocol | `~/L-Projects/t3UI-refs/t3code-upstream` (upstream main) | Read-only. Users run `npx t3@nightly`, so the client must speak upstream's protocol. |
| GPUI / gpui-kit | `~/L-Projects/t3UI-refs/gpui-kit`, skill in `.claude/skills/gpui-kit` | Never invent an API: grep the source for real signatures. |
| Specs | `docs/spec/*.md` | Distilled from the sources above, with `path:line` pointers. |
| Reference screenshots | `docs/reference/*.png` | The fork's UI captured at 1440x900 @2x. |

When a spec and the source disagree, the source wins; fix the spec.

The fork's `main` is upstream t3code `b33eda13` plus the user's UI polish, so the fork source and
the upstream protocol source agree. UI specs written before 2026-10-02 described the stale July
checkout; each has a "Refreshed against fe7d3092c" note once updated. Until then, trust the source.

## Crates

- `t3-protocol`: serde wire types + Effect RPC frames. No IO, no GPUI.
- `t3-client`: tokio networking, environment catalog, auth/pairing, T3 Connect, reducers. No GPUI.
  Exposes executor-agnostic handles (channels/futures) that GPUI tasks can await.
- `t3-ui`: theme tokens, fonts, icons, styled primitives reproducing the fork's components.
- `t3-highlight`: syntect highlighting with the fork's Shiki `pierre-dark/light` colors and
  language resolution. No GPUI. `tests/shiki_parity.rs` measures agreement with Shiki.
- `t3-markdown`: chat markdown (`Markdown` entity) matching `ChatMarkdown`: parser, streaming
  chunks, inline layout element, code/table chrome. Parser/chunker build without GPUI
  (`--no-default-features`).
- `t3-app`: the window shell and views. `lib.rs` exposes views so headless snapshot rendering can
  mount them; `main.rs` only boots.

## Rules

- Use upstream's wire shapes. Decoders must tolerate unknown fields and unknown `_tag` variants
  (the server ships nightly). Never panic on server data.
- Styling values come from `t3-ui` tokens, not ad-hoc colors. Tailwind: 1 unit = 4px,
  `text-xs` = 12/16, `text-sm` = 14/20, `text-base` = 16/24.
- No continuously repainting animations (spinners, pulses, shimmer) unless the fork has them
  and there is no static alternative. If unavoidable, keep them small and stop them offscreen.
- Prefer end-to-end verification through the real app and server. Only write isolated tests for
  pure logic where failure modes are listed first (protocol decoding, reducers).
- Comments: a short doc comment on types/functions explaining how they are used. Keep in sync.
- Commits: lowercase conventional commits (`feat(chat): render work log rows`). No co-author trailers.

## Build directories (important)

Never share a `CARGO_TARGET_DIR` between worktrees. Cargo hashes workspace crates by their path
relative to the workspace root, so `t3-client` in two worktrees maps to the same artifact and
fingerprint: a build can silently link another checkout's code and report it fresh. Each
worktree uses its own dir:

```sh
export PATH=$HOME/.cargo/bin:$PATH CARGO_TARGET_DIR=$HOME/L-Projects/t3UI-targets/<worktree-name> \
  CARGO_BUILD_JOBS=4 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
```

## Landing on main

Run `script/preland.sh` before every push to main and only push if it prints `preland: ok`.
It runs rustfmt, Linux clippy + tests for GPUI-free crates, and macOS clippy for the workspace.
A red main breaks every other agent's CI signal.

## Commands

```sh
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run                     # opens the app (Linux needs the GPUI system deps)
script/check-macos.sh -p t3-app   # type-check for macOS from Linux (patched gpui-pre-apple, check only)
```

CI (`.github/workflows/ci.yml`, every push to main and on manual dispatch) runs Linux clippy +
tests, and one macOS job that renders the headless snapshot scenes and smoke-launches the app; the
PNGs are uploaded as the `snapshots` artifact. Release DMGs are built only by
`.github/workflows/release.yml` (on a `v*` tag or `gh workflow run release.yml -f tag=...`) and
published as a GitHub Release.
