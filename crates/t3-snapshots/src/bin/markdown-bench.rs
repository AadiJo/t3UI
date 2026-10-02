//! Streams a long assistant message into a [`t3_markdown::Markdown`] and times each frame, with
//! block caching off and on, with the message pinned to the top of the window (nothing moves
//! while it grows) and to the bottom (the timeline following the tail: everything above moves up
//! whenever the tail gains a line).
//!
//! macOS only (GPUI's headless renderer and CoreText shaping):
//! `cargo run --release -p t3-snapshots --bin markdown-bench` (CI: the Markdown bench workflow)

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("markdown-bench: needs macOS (GPUI headless rendering)");
}

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    bench::run()
}

#[cfg(target_os = "macos")]
mod bench {
    use std::{sync::Arc, time::Instant};

    use gpui_kit::{
        AppContext as _, Bounds, Context, Entity, HeadlessAppContext, IntoElement,
        ParentElement as _, Render, Styled as _, Window, WindowBounds, WindowOptions,
        base::TextSelectionLayer, div, prelude::FluentBuilder as _, px, size,
    };
    use t3_markdown::{Markdown, MarkdownOptions};
    use t3_ui::{ActiveColors as _, ThemeMode};

    /// Bytes per streamed update.
    const DELTA: usize = 24;

    /// A long, typical assistant answer: sections with prose, inline code, links, lists,
    /// code blocks and the odd table. No footnotes, raw HTML or task lists, which keep the fork's
    /// chunker from freezing anything after them.
    fn message() -> String {
        let languages = ["ts", "rust", "python", "bash"];
        let mut out = String::new();
        for section in 0..28 {
            out.push_str(&format!(
                "## Step {section}: update the `module_{section}` wiring\n\n"
            ));
            out.push_str(&format!(
                "The **{section}th** change moves the `TurnTracker` state into a shared store so the \
                 [timeline](https://github.com/AadiJo/t3UI) can read it without cloning. It keeps \
                 the existing `subscribe` contract and only touches `crates/t3-app/src/step_{section}.rs`, \
                 which is why the diff stays small even though the behaviour changes noticeably.\n\n"
            ));
            out.push_str(
                "- Keep the reducer pure and test it with recorded frames\n\
                 - Move the GPUI task into the view that owns the entity\n\
                 - Drop the extra `Arc` around the shared state\n\n",
            );
            let language = languages[section % languages.len()];
            out.push_str(&format!("```{language}\n"));
            for line in 0..10 {
                out.push_str(&match language {
                    "ts" => format!(
                        "  const value{line} = await store.get(\"key-{line}\"); // step {section}\n"
                    ),
                    "rust" => format!(
                        "    let value_{line} = store.get(\"key-{line}\")?; // step {section}\n"
                    ),
                    "python" => format!(
                        "    value_{line} = await store.get(\"key-{line}\")  # step {section}\n"
                    ),
                    _ => format!(
                        "cargo test -p t3-app step_{section}_{line} && echo \"ok {line}\"\n"
                    ),
                });
            }
            out.push_str("```\n\n");
            if section % 5 == 4 {
                out.push_str(
                    "| Crate | Change | Lines |\n| :-- | :-- | --: |\n\
                     | t3-app | moved state | 42 |\n| t3-client | new reducer | 118 |\n\
                     | t3-ui | no change | 0 |\n\n",
                );
            }
        }
        out
    }

    struct Root {
        markdown: Entity<Markdown>,
        bottom: bool,
    }

    impl Render for Root {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .bg(cx.colors().background)
                .child(TextSelectionLayer)
                .child(
                    div()
                        .mx_auto()
                        .w(px(768.))
                        .h_full()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .when(self.bottom, |this| this.justify_end())
                        .child(
                            div()
                                .flex_none()
                                .px(px(4.))
                                .py(px(2.))
                                .child(self.markdown.clone()),
                        ),
                )
        }
    }

    struct Stats {
        frames: Vec<f64>,
        parses: Vec<f64>,
        finish: f64,
    }

    fn percentile(sorted: &[f64], p: f64) -> f64 {
        sorted[((sorted.len() - 1) as f64 * p).round() as usize]
    }

    fn summary(values: &[f64]) -> String {
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
        format!(
            "mean {mean:6.2}  p50 {:6.2}  p90 {:6.2}  max {:6.2}",
            percentile(&sorted, 0.5),
            percentile(&sorted, 0.9),
            sorted[sorted.len() - 1]
        )
    }

    fn stream(text: &str, bottom: bool, caching: bool) -> anyhow::Result<Stats> {
        let mut cx = HeadlessAppContext::with_platform(
            gpui_kit::platform::current_platform(true).text_system(),
            Arc::new(t3_ui::Assets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(|cx| {
            gpui_kit::init(cx);
            t3_ui::init(ThemeMode::Dark, cx);
        });
        let (handle, markdown) = cx.update(|cx| {
            let markdown = cx.new(|cx| {
                let mut markdown = Markdown::new("", MarkdownOptions::default(), cx);
                markdown.set_block_caching(caching, cx);
                markdown
            });
            let root_markdown = markdown.clone();
            let handle = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Default::default(),
                        size: size(px(1440.), px(900.)),
                    })),
                    focus: false,
                    show: false,
                    ..Default::default()
                },
                cx,
                move |_, cx| {
                    cx.new(|_| Root {
                        markdown: root_markdown,
                        bottom,
                    })
                },
            );
            handle.map(|(handle, _)| (handle, markdown))
        })?;
        let draw = |cx: &mut HeadlessAppContext| -> anyhow::Result<f64> {
            let started = Instant::now();
            cx.update_window(handle, |_, window, cx| window.draw(cx).clear(cx))?;
            let elapsed = started.elapsed().as_secs_f64() * 1000.;
            // Background highlighting and deferred work, outside the timed frame.
            cx.run_until_parked();
            Ok(elapsed)
        };
        draw(&mut cx)?;

        let mut frames = Vec::new();
        let mut parses = Vec::new();
        let mut end = 0;
        while end < text.len() {
            end = (end + DELTA).min(text.len());
            while !text.is_char_boundary(end) {
                end += 1;
            }
            let prefix = text[..end].to_string();
            let started = Instant::now();
            cx.update(|cx| markdown.update(cx, |markdown, cx| markdown.set_text(prefix, true, cx)));
            parses.push(started.elapsed().as_secs_f64() * 1000.);
            frames.push(draw(&mut cx)?);
        }
        cx.update(|cx| markdown.update(cx, |markdown, cx| markdown.set_streaming(false, cx)));
        let finish = draw(&mut cx)?;
        Ok(Stats {
            frames,
            parses,
            finish,
        })
    }

    pub fn run() -> anyhow::Result<()> {
        let text = message();
        println!(
            "message: {} KB, {} updates of {DELTA} bytes",
            text.len() / 1024,
            text.len().div_ceil(DELTA)
        );
        for bottom in [false, true] {
            for caching in [false, true] {
                let stats = stream(&text, bottom, caching)?;
                let tail = stats.frames.len() * 3 / 4;
                println!(
                    "\n{}, block caching {}:",
                    if bottom {
                        "bottom-pinned"
                    } else {
                        "top-pinned"
                    },
                    if caching { "on" } else { "off" }
                );
                println!("  frame ms, all updates:   {}", summary(&stats.frames));
                println!(
                    "  frame ms, last quarter:  {}",
                    summary(&stats.frames[tail..])
                );
                println!("  set_text ms, all:        {}", summary(&stats.parses));
                println!(
                    "  frame ms total: {:.0}, finishing frame: {:.2}",
                    stats.frames.iter().sum::<f64>(),
                    stats.finish
                );
            }
        }
        Ok(())
    }
}
