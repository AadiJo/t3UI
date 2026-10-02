//! Light/dark/system theme selection.
//!
//! [`Theme`] is a GPUI global holding the user's [`ThemeMode`] and the resolved
//! [`Appearance`]. Components read colors with `cx.colors()` ([`ActiveColors`]). Changing
//! the mode also re-themes gpui-component (inputs, lists, scrollbars, popover positioning)
//! from the generated ThemeSet in `gpui_component.json`, with its `Root` background made
//! transparent so the window glass shows through.

use gpui_kit::{
    App, Global, SharedString, Subscription, Window, WindowAppearance,
    component::{Theme as ComponentTheme, ThemeRegistry, ThemeToken},
    transparent_black,
};

use crate::tokens::{Colors, DARK, LIGHT};

/// gpui-component ThemeSet generated from tokens.json by `tools/gen_tokens.py`.
const COMPONENT_THEMES: &str = include_str!("gpui_component.json");
const LIGHT_THEME_NAME: &str = "T3 Code Light";
const DARK_THEME_NAME: &str = "T3 Code Dark";

/// The user's theme preference (`localStorage["t3code:theme"]` on the web). Persist it;
/// the default follows the system appearance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

impl ThemeMode {
    /// Resolves the preference against the current system appearance.
    pub fn resolve(self, system: WindowAppearance) -> Appearance {
        match self {
            Self::Light => Appearance::Light,
            Self::Dark => Appearance::Dark,
            Self::System => system.into(),
        }
    }
}

/// The appearance actually painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Appearance {
    Light,
    Dark,
}

impl Appearance {
    /// The color set for this appearance.
    pub fn colors(self) -> &'static Colors {
        match self {
            Self::Light => &LIGHT,
            Self::Dark => &DARK,
        }
    }

    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }
}

impl From<WindowAppearance> for Appearance {
    fn from(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
        }
    }
}

/// Global theme state. Created by [`crate::init`]; change it with [`set_mode`].
pub struct Theme {
    mode: ThemeMode,
    appearance: Appearance,
    mono_family: SharedString,
    native_appearance: bool,
}

impl Global for Theme {}

impl Theme {
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    /// The user's preference.
    pub fn mode(&self) -> ThemeMode {
        self.mode
    }

    /// The resolved appearance.
    pub fn appearance(&self) -> Appearance {
        self.appearance
    }

    /// The active color set.
    pub fn colors(&self) -> &'static Colors {
        self.appearance.colors()
    }

    /// The monospace family chosen at startup (see [`crate::fonts::mono_family`]).
    pub fn mono_family(&self) -> &SharedString {
        &self.mono_family
    }
}

/// Access to the active colors from any context: `cx.colors().border`.
pub trait ActiveColors {
    fn colors(&self) -> &'static Colors;
}

impl ActiveColors for App {
    #[inline]
    fn colors(&self) -> &'static Colors {
        Theme::global(self).colors()
    }
}

/// Installs the theme global and gpui-component's T3 themes. `mode` is the persisted
/// preference; `System` resolves against the app's current appearance.
pub(crate) fn init(mode: ThemeMode, mono_family: SharedString, cx: &mut App) {
    if let Err(error) = ThemeRegistry::global_mut(cx).load_themes_from_str(COMPONENT_THEMES) {
        tracing::error!("failed to load gpui-component themes: {error:#}");
    }
    let appearance = mode.resolve(cx.window_appearance());
    cx.set_global(Theme {
        mode,
        appearance,
        mono_family,
        native_appearance: false,
    });
    apply(cx);
}

/// Changes the theme preference and repaints every window. The switch is instant, like the
/// web's one-frame `.no-transitions`.
pub fn set_mode(mode: ThemeMode, cx: &mut App) {
    let appearance = mode.resolve(cx.window_appearance());
    let theme = cx.global_mut::<Theme>();
    theme.mode = mode;
    theme.appearance = appearance;
    apply(cx);
}

/// Re-resolves a `System` preference whenever this window's appearance changes. Keep the
/// returned subscription alive for the window's lifetime.
pub fn observe_system_appearance(window: &mut Window, cx: &mut App) -> Subscription {
    // The window may already disagree with the app-level value read at init.
    sync_system_appearance(window, cx);
    window.observe_window_appearance(sync_system_appearance)
}

fn sync_system_appearance(window: &mut Window, cx: &mut App) {
    let theme = Theme::global(cx);
    if theme.mode != ThemeMode::System {
        return;
    }
    let appearance = Appearance::from(window.appearance());
    if appearance != theme.appearance {
        cx.global_mut::<Theme>().appearance = appearance;
        apply(cx);
    }
}

/// Makes theme changes also set `NSApp.appearance`, so the native window material and
/// traffic lights follow an explicit light/dark preference. Only for real windows; headless
/// snapshot contexts leave it off.
pub fn enable_native_appearance(cx: &mut App) {
    cx.global_mut::<Theme>().native_appearance = true;
    apply_native_appearance(cx.global::<Theme>().mode);
}

/// Pushes the resolved appearance into gpui-component and refreshes every window.
fn apply(cx: &mut App) {
    let theme = Theme::global(cx);
    let (dark, mono_family, native, mode) = (
        theme.appearance.is_dark(),
        theme.mono_family.clone(),
        theme.native_appearance,
        theme.mode,
    );
    let name = if dark {
        DARK_THEME_NAME
    } else {
        LIGHT_THEME_NAME
    };
    let config = ThemeRegistry::global(cx).themes().get(name).cloned();

    ComponentTheme::update(cx, |component| {
        if let Some(config) = &config {
            component.apply_config(config);
        }
        component.mono_font_family = mono_family;
        // We draw our own focus rings (spec section 1.6); theirs is a 3px ring/50 band.
        component.focus_ring = false;
        // `Root` (and Dialog, Sheet, TabBar) paint `tokens.background`. Keep the solid
        // color for components that read it, but paint nothing so the glass shows.
        // `reconcile` keeps this token because its color matches `colors.background`.
        component.tokens.background = ThemeToken {
            color: component.background,
            background: transparent_black().into(),
        };
    });

    if native {
        apply_native_appearance(mode);
    }
}

#[cfg(target_os = "macos")]
fn apply_native_appearance(mode: ThemeMode) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
    };

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: the appearance name statics are provided by AppKit and live for the process.
    let name = match mode {
        ThemeMode::Light => Some(unsafe { NSAppearanceNameAqua }),
        ThemeMode::Dark => Some(unsafe { NSAppearanceNameDarkAqua }),
        ThemeMode::System => None,
    };
    let appearance = name.and_then(NSAppearance::appearanceNamed);
    app.setAppearance(appearance.as_deref());
}

#[cfg(not(target_os = "macos"))]
fn apply_native_appearance(_mode: ThemeMode) {}
