//! The sidebar header's stage art (`SidebarStageBackdrop.tsx`): a night sky behind the top of
//! the sidebar while the primary server is a Nightly build. Static: no animation.
//!
//! The fork draws an SVG with a `0 0 8192 96` viewBox scaled to the 80px header art, so widening
//! the sidebar reveals more canvas. Here the same SVG (with the CSS mask and fade overlay baked
//! in) is rendered once per appearance to an image wide enough for any sidebar width.

use std::sync::{Arc, OnceLock};

use gpui_kit::{
    App, Hsla, Image, ImageFormat, IntoElement, ParentElement as _, RenderOnce, Rgba, Styled as _,
    Window, div, img, px,
};
use t3_ui::{ActiveColors as _, Colors};

use crate::state::AppState;

/// Which art the header shows. The native app has no dev channel, so only the server-backed
/// Nightly stage applies (`resolveServerBackedAppStageLabel`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageBackdrop {
    Nightly,
}

/// The art behind the sidebar header, if any: the primary server runs a nightly build and the
/// environment identification mode is `artwork` (the default; the setting is not ported yet).
pub fn stage_backdrop(state: &AppState, cx: &App) -> Option<StageBackdrop> {
    let environment = state.primary_environment()?.read(cx);
    let version = &environment.config()?.environment.server_version;
    is_nightly_server_version(version).then_some(StageBackdrop::Nightly)
}

/// `NIGHTLY_SERVER_VERSION_PATTERN`: `^[^-+]+-(?:nightly|preview)\.\d{8}\.\d+$`, e.g.
/// `0.0.45-nightly.20261002.2561`.
pub fn is_nightly_server_version(version: &str) -> bool {
    let Some((base, rest)) = version.split_once('-') else {
        return false;
    };
    if base.is_empty() || base.contains('+') {
        return false;
    }
    let Some(rest) = rest
        .strip_prefix("nightly.")
        .or_else(|| rest.strip_prefix("preview."))
    else {
        return false;
    };
    let Some((date, build)) = rest.split_once('.') else {
        return false;
    };
    let digits = |value: &str| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit());
    date.len() == 8 && digits(date) && digits(build)
}

/// Art height in px (`h-20`); the SVG's 96 units map onto it.
const ART_HEIGHT: f32 = 80.;
/// SVG units rendered: 2400 units = 2000px, wider than any sidebar (max = window - 640).
const ART_UNITS: f32 = 2400.;

/// `div.sidebar-stage-backdrop`: absolute, top 0, full width, 80px, clipped, behind the
/// header row and the search row.
#[derive(IntoElement)]
pub struct StageBackdropArt(pub StageBackdrop);

impl RenderOnce for StageBackdropArt {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let StageBackdrop::Nightly = self.0;
        let image = nightly_image(cx.sidebar_colors());
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(ART_HEIGHT))
            .overflow_hidden()
            .child(
                img(image)
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(ART_UNITS * ART_HEIGHT / 96.))
                    .h(px(ART_HEIGHT)),
            )
    }
}

fn nightly_image(colors: &'static Colors) -> Arc<Image> {
    static DARK: OnceLock<Arc<Image>> = OnceLock::new();
    static LIGHT: OnceLock<Arc<Image>> = OnceLock::new();
    let cell = if colors.is_dark { &DARK } else { &LIGHT };
    cell.get_or_init(|| {
        Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            nightly_svg(colors).into_bytes(),
        ))
    })
    .clone()
}

/// `#rrggbb` for an SVG attribute (alpha goes in the matching `*-opacity`).
fn hex(color: Hsla) -> String {
    let rgba = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(rgba.r),
        channel(rgba.g),
        channel(rgba.b)
    )
}

/// (x, y, r, opacity) of `NIGHTLY_STARS`.
const STARS: [(f32, f32, f32, f32); 17] = [
    (14., 10., 0.6, 0.85),
    (38., 22., 0.4, 0.55),
    (58., 8., 0.5, 0.7),
    (84., 16., 0.4, 0.5),
    (104., 7., 0.6, 0.8),
    (126., 20., 0.4, 0.55),
    (148., 11., 0.5, 0.7),
    (170., 24., 0.4, 0.5),
    (192., 9., 0.6, 0.8),
    (214., 18., 0.4, 0.55),
    (236., 8., 0.5, 0.7),
    (258., 20., 0.45, 0.6),
    (278., 11., 0.55, 0.75),
    (26., 34., 0.4, 0.45),
    (118., 34., 0.4, 0.45),
    (202., 32., 0.4, 0.5),
    (268., 34., 0.4, 0.45),
];

/// `NIGHTLY_SPARKLES`.
const SPARKLES: [(f32, f32); 3] = [(70., 28.), (160., 36.), (246., 26.)];

/// `NightlySkyArt`, rendered at 2x, with the `sidebar-stage-backdrop` mask (opaque to 55%,
/// transparent at 92%) and its `::after` fade to `--sidebar-stage-fade` baked in.
fn nightly_svg(colors: &Colors) -> String {
    let stars: String = STARS
        .iter()
        .map(|(cx, cy, r, opacity)| {
            format!(r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill-opacity="{opacity}"/>"#)
        })
        .collect();
    let sparkles: String = SPARKLES
        .iter()
        .map(|(x, y)| {
            format!(
                r#"<path d="M{} {y}H{}"/><path d="M{x} {}V{}"/>"#,
                x - 1.5,
                x + 1.5,
                y - 1.5,
                y + 1.5
            )
        })
        .collect();
    let fade = hex(colors.sidebar_stage_fade);
    let fade_stops: String = [
        (0., 0.),
        (0.28, 0.),
        (0.40, 0.10),
        (0.52, 0.30),
        (0.64, 0.58),
        (0.75, 0.82),
        (0.85, 0.96),
        (0.93, 1.),
        (1., 1.),
    ]
    .iter()
    .map(|(offset, opacity)| {
        format!(r#"<stop offset="{offset}" stop-color="{fade}" stop-opacity="{opacity}"/>"#)
    })
    .collect();
    let units = ART_UNITS;
    let width = units * ART_HEIGHT / 96. * 2.;
    let height = ART_HEIGHT * 2.;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {units} 96" fill="none">
<defs>
<linearGradient id="sky" x1="24" y1="0" x2="264" y2="96" gradientUnits="userSpaceOnUse" spreadMethod="reflect">
<stop stop-color="{bottom}"/><stop offset="0.5" stop-color="{mid}"/><stop offset="1" stop-color="{top}"/>
</linearGradient>
<radialGradient id="glow" cx="0" cy="0" r="1" gradientTransform="translate(216 18) rotate(137) scale(120 84)" gradientUnits="userSpaceOnUse">
<stop stop-color="{glow_highlight}" stop-opacity="0.4"/><stop offset="0.5" stop-color="{glow_secondary}" stop-opacity="0.16"/><stop offset="1" stop-color="{bottom}" stop-opacity="0"/>
</radialGradient>
<linearGradient id="cloud" x1="0" y1="60" x2="288" y2="96" gradientUnits="userSpaceOnUse">
<stop stop-color="{highlight}" stop-opacity="0.5"/><stop offset="0.52" stop-color="{secondary}" stop-opacity="0.62"/><stop offset="1" stop-color="{tertiary}" stop-opacity="0.5"/>
</linearGradient>
<filter id="soft" x="-24" y="-24" width="336" height="144" filterUnits="userSpaceOnUse"><feGaussianBlur stdDeviation="4"/></filter>
<pattern id="stars" width="288" height="96" patternUnits="userSpaceOnUse">
<g fill="{line}">{stars}</g>
<g stroke="{sparkle}" stroke-linecap="round" stroke-opacity="0.7" stroke-width="0.6">{sparkles}</g>
</pattern>
<pattern id="glows" width="640" height="96" patternUnits="userSpaceOnUse"><rect width="640" height="96" fill="url(#glow)"/></pattern>
<linearGradient id="fade" x1="0" y1="0" x2="0" y2="96" gradientUnits="userSpaceOnUse">{fade_stops}</linearGradient>
<linearGradient id="maskfill" x1="0" y1="0" x2="0" y2="96" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="#fff"/><stop offset="0.55" stop-color="#fff"/><stop offset="0.92" stop-color="#000"/><stop offset="1" stop-color="#000"/>
</linearGradient>
<mask id="mask" maskUnits="userSpaceOnUse" x="0" y="0" width="{units}" height="96"><rect width="{units}" height="96" fill="url(#maskfill)"/></mask>
</defs>
<g mask="url(#mask)">
<rect width="{units}" height="96" fill="url(#sky)"/>
<rect width="{units}" height="96" fill="url(#glows)"/>
<rect width="{units}" height="96" fill="url(#stars)"/>
<g filter="url(#soft)"><path d="M-12 88C-12 74 0 63 14 63C18 50 30 41 44 41C58 41 70 49 74 62C79 57 86 54 94 54C110 54 123 66 124 82C132 83 138 88 141 96H-12V88Z" fill="url(#cloud)"/></g>
<g filter="url(#soft)"><path d="M150 96C151 84 161 75 173 75C176 64 186 57 198 57C210 57 220 64 223 75C231 75 238 80 241 87C250 87 257 91 260 96H150Z" fill="url(#cloud)" fill-opacity="0.8"/></g>
<rect width="{units}" height="96" fill="url(#fade)"/>
</g>
</svg>"##,
        bottom = hex(colors.stage_night_bottom),
        mid = hex(colors.stage_night_mid),
        top = hex(colors.stage_night_top),
        glow_highlight = hex(colors.stage_night_glow_highlight),
        glow_secondary = hex(colors.stage_night_glow_secondary),
        highlight = hex(colors.stage_night_highlight),
        secondary = hex(colors.stage_night_secondary),
        tertiary = hex(colors.stage_night_tertiary),
        line = hex(colors.stage_night_line),
        sparkle = hex(colors.stage_night_sparkle),
    )
}

#[cfg(test)]
mod tests {
    //! Failure modes: a stable release, a dev build, or a build-metadata suffix must not read as
    //! Nightly; `preview` builds do.
    use super::is_nightly_server_version;

    #[test]
    fn nightly_versions_match_the_fork_pattern() {
        assert!(is_nightly_server_version("0.0.45-nightly.20261002.2561"));
        assert!(is_nightly_server_version("1.2.3-preview.20260101.1"));
        assert!(!is_nightly_server_version("0.0.45"));
        assert!(!is_nightly_server_version("0.0.45-nightly.2026100.1"));
        assert!(!is_nightly_server_version("0.0.45-nightly.20261002"));
        assert!(!is_nightly_server_version("0.0.45-beta.20261002.1"));
        assert!(!is_nightly_server_version("0.0.45+x-nightly.20261002.1"));
        assert!(!is_nightly_server_version("-nightly.20261002.1"));
    }
}
