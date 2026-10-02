# T3UI

A native port of the [T3 Code](https://github.com/pingdotgg/t3code) desktop client, built with
[GPUI](https://gpui.rs) and [gpui-kit](https://gpui-kit.com). It connects to T3 Code servers
(`npx t3`) and to environments linked through T3 Connect.

Status: early development.

## Build

```sh
cargo run --release
```

macOS DMGs are built by CI on every push (see the `T3UI-macos-arm64` artifact).
