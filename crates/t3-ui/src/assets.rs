//! The app's [`AssetSource`]: bundled fonts and icons from the repo's `assets/` directory,
//! falling back to gpui-kit's own icon bundle for paths we don't ship.
//!
//! Pass it to `gpui_kit::application().with_assets(t3_ui::Assets)` and to headless
//! contexts so snapshots render the same glyphs and icons as the app.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

#[derive(rust_embed::RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../assets"]
#[include = "fonts/*.ttf"]
#[include = "icons/**/*.svg"]
pub(crate) struct Embedded;

/// Asset source for every T3UI window. Paths look like `icons/lucide/x.svg`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(file) = Embedded::get(path) {
            return Ok(Some(file.data));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut entries: Vec<SharedString> = Embedded::iter()
            .filter(|name| name.starts_with(path))
            .map(|name| SharedString::from(name.into_owned()))
            .collect();
        entries.extend(gpui_kit::assets::Assets.list(path)?);
        Ok(entries)
    }
}
