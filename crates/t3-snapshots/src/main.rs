//! Renders named scenes of the real app views to PNG files without a display.
//!
//! Usage: `cargo run -p t3-snapshots -- <out-dir> [scene ...]`. Only macOS has GPUI's headless
//! Metal renderer; elsewhere this exits with a message. CI uploads the PNGs as an artifact.

fn main() -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    return macos::run();
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("t3-snapshots: GPUI has no headless renderer on this platform");
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{path::PathBuf, sync::Arc};

    use gpui_kit::{
        AppContext as _, Bounds, HeadlessAppContext, WindowBounds, WindowOptions, px, size,
        test::TestWindowExt as _,
    };

    /// Window size of every scene, matching the web reference captures.
    const WIDTH: f32 = 1440.;
    const HEIGHT: f32 = 900.;

    struct Scene {
        name: &'static str,
        build: fn(&mut gpui_kit::Window, &mut gpui_kit::App) -> gpui_kit::AnyView,
    }

    fn scenes() -> Vec<Scene> {
        vec![Scene {
            name: "workspace-empty-dark",
            build: |window, cx| cx.new(|cx| t3_app::Workspace::new(window, cx)).into(),
        }]
    }

    pub fn run() -> anyhow::Result<()> {
        let mut args = std::env::args().skip(1);
        let out_dir = PathBuf::from(args.next().unwrap_or_else(|| "snapshots".into()));
        let only: Vec<String> = args.collect();
        std::fs::create_dir_all(&out_dir)?;

        for scene in scenes() {
            if !only.is_empty() && !only.iter().any(|name| name == scene.name) {
                continue;
            }
            let mut cx = HeadlessAppContext::with_platform(
                gpui_kit::platform::current_platform(true).text_system(),
                Arc::new(gpui_kit::assets::Assets),
                gpui_kit::platform::current_headless_renderer,
            );
            cx.update(gpui_kit::init);
            let build = scene.build;
            let (handle, _) = cx.update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: Default::default(),
                            size: size(px(WIDTH), px(HEIGHT)),
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
            cx.update_window(handle, |_, window, cx| window.render_frame(cx))?;
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
