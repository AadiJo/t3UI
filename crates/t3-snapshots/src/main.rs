//! Renders named scenes of the real app views to PNG files without a display.
//!
//! Usage: `cargo run -p t3-snapshots -- <out-dir> [scene ...]`. Only macOS has GPUI's headless
//! Metal renderer; elsewhere this lists the scenes and exits. CI uploads the PNGs as an
//! artifact. Scenes live in `src/scenes/<name>.rs` and are registered in `scenes::all`.

mod scenes;

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let out_dir = PathBuf::from(args.next().unwrap_or_else(|| "snapshots".into()));
    let only: Vec<String> = args.collect();
    let scenes: Vec<scenes::Scene> = scenes::all()
        .into_iter()
        .filter(|scene| only.is_empty() || only.iter().any(|name| name == scene.name))
        .collect();

    #[cfg(target_os = "macos")]
    return macos::run(&out_dir, scenes);

    #[cfg(not(target_os = "macos"))]
    {
        eprintln!(
            "t3-snapshots: GPUI has no headless renderer on this platform; would write {} scenes to {}:",
            scenes.len(),
            out_dir.display()
        );
        for scene in scenes {
            eprintln!("  {}", scene.name);
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{path::Path, sync::Arc};

    use gpui_kit::{
        AppContext as _, Bounds, HeadlessAppContext, WindowBounds, WindowOptions, px, size,
        test::TestWindowExt as _,
    };

    use crate::scenes::Scene;

    pub fn run(out_dir: &Path, scenes: Vec<Scene>) -> anyhow::Result<()> {
        std::fs::create_dir_all(out_dir)?;
        for scene in scenes {
            let mut cx = HeadlessAppContext::with_platform(
                gpui_kit::platform::current_platform(true).text_system(),
                Arc::new(t3_ui::Assets),
                gpui_kit::platform::current_headless_renderer,
            );
            cx.update(|cx| {
                gpui_kit::init(cx);
                t3_ui::init(scene.theme, cx);
            });
            let build = scene.build;
            let (width, height) = scene.size;
            let (handle, _) = cx.update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: Default::default(),
                            size: size(px(width), px(height)),
                        })),
                        focus: false,
                        show: false,
                        ..Default::default()
                    },
                    cx,
                    |window, cx| {
                        let view = build(window, cx);
                        cx.new(|_| Host(view))
                    },
                )
            })?;
            // Images (logos, the noise tile) decode on the background executor: draw, let
            // them load, then draw again so they are in the capture.
            cx.update_window(handle, |_, window, cx| window.render_frame(cx))?;
            cx.run_until_parked();
            cx.update_window(handle, |_, window, cx| {
                window.refresh();
                window.render_frame(cx)
            })?;
            let image = cx.capture_screenshot(handle)?;
            let path = out_dir.join(format!("{}.png", scene.name));
            image.save(&path)?;
            println!("{} ({}x{})", path.display(), image.width(), image.height());
        }
        Ok(())
    }

    /// Wraps an arbitrary view so every scene can be opened through `gpui_kit::open_window`.
    struct Host(gpui_kit::AnyView);

    impl gpui_kit::Render for Host {
        fn render(
            &mut self,
            _: &mut gpui_kit::Window,
            _: &mut gpui_kit::Context<Self>,
        ) -> impl gpui_kit::IntoElement {
            self.0.clone()
        }
    }
}
