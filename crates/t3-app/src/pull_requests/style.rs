//! The page's tones and glyphs (`pullRequestIcons.tsx`, `pullRequestPresentation.tsx`), from
//! t3-ui tokens and the Tailwind palette.

use gpui_kit::Hsla;
use t3_logic::pull_requests::StateKey;
use t3_protocol::pull_requests::{ChecksState, ReviewDecision, SourceControlProviderKind};
use t3_ui::{Colors, IconName, Logo, tokens::palette};

/// The lifecycle glyph (`PullRequestGlyph`).
pub fn state_icon(state: StateKey) -> IconName {
    match state {
        StateKey::Open => IconName::GitPullRequestArrow,
        StateKey::Draft => IconName::GitPullRequestDraft,
        StateKey::Closed => IconName::GitPullRequestClosed,
        StateKey::Merged => IconName::GitMerge,
    }
}

fn pick(colors: &Colors, light: Hsla, dark: Hsla) -> Hsla {
    if colors.is_dark { dark } else { light }
}

/// The lifecycle tone (`PULL_REQUEST_STATE_PRESENTATION`): emerald, zinc, red, violet.
pub fn state_tone(state: StateKey, colors: &Colors) -> Hsla {
    match state {
        StateKey::Open => pick(
            colors,
            palette::EMERALD.s600,
            palette::EMERALD.s300.opacity(0.9),
        ),
        StateKey::Draft => pick(colors, palette::ZINC.s500, palette::ZINC.s400.opacity(0.8)),
        StateKey::Closed => pick(colors, palette::RED.s600, palette::RED.s300.opacity(0.9)),
        StateKey::Merged => pick(
            colors,
            palette::VIOLET.s600,
            palette::VIOLET.s300.opacity(0.9),
        ),
    }
}

/// The checks rollup glyph and tone (`CHECKS_STATE_PRESENTATION`).
pub fn checks_glyph(state: &ChecksState, colors: &Colors) -> (IconName, Hsla) {
    match state {
        ChecksState::Passing => (
            IconName::CircleCheck,
            pick(
                colors,
                palette::EMERALD.s600,
                palette::EMERALD.s300.opacity(0.9),
            ),
        ),
        ChecksState::Failing => (IconName::CircleX, colors.destructive),
        _ => (
            IconName::CircleDot,
            pick(
                colors,
                palette::AMBER.s600,
                palette::AMBER.s400.opacity(0.9),
            ),
        ),
    }
}

/// The review verdict glyph and tone (`reviewDecisionPresentation`).
pub fn review_glyph(decision: &ReviewDecision, colors: &Colors) -> (IconName, Hsla) {
    match decision {
        ReviewDecision::Approved => (
            IconName::UserCheck,
            pick(
                colors,
                palette::EMERALD.s600,
                palette::EMERALD.s300.opacity(0.9),
            ),
        ),
        ReviewDecision::ChangesRequested => (
            IconName::UserRoundX,
            pick(
                colors,
                palette::AMBER.s600.opacity(0.9),
                palette::AMBER.s400.opacity(0.8),
            ),
        ),
        _ => (IconName::UserRound, colors.muted_foreground_60),
    }
}

/// The provider's mark (`getSourceControlPresentationForKind(...).Icon`).
pub fn provider_logo(kind: &SourceControlProviderKind) -> Option<Logo> {
    Some(match kind {
        SourceControlProviderKind::Github => Logo::GitHubIcon,
        SourceControlProviderKind::Gitlab => Logo::GitLabIcon,
        SourceControlProviderKind::Forgejo => Logo::ForgejoIcon,
        SourceControlProviderKind::AzureDevops => Logo::AzureDevOpsIcon,
        SourceControlProviderKind::Bitbucket => Logo::BitbucketIcon,
        _ => return None,
    })
}

/// `--diff-addition-foreground`: emerald-600 / emerald-400.
pub fn diff_addition(colors: &Colors) -> Hsla {
    colors.diff_addition_foreground
}

/// `--diff-deletion`.
pub fn diff_deletion(colors: &Colors) -> Hsla {
    colors.diff_deletion
}

/// The `label` badge variant: the label color at 8% / 12% behind, and a mix of it with the
/// foreground (30% light, 45% dark) for the text.
pub fn label_chip_colors(rgb: u32, colors: &Colors) -> (Hsla, Hsla) {
    let label = t3_ui::tokens::hex((rgb << 8) | 0xFF);
    let (wash, share) = if colors.is_dark {
        (0.12, 0.45)
    } else {
        (0.08, 0.30)
    };
    (
        label.opacity(wash),
        mix_srgb(label, colors.foreground, share),
    )
}

/// `color-mix(in srgb, a share, b)`.
pub fn mix_srgb(a: Hsla, b: Hsla, share: f32) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    let mix = |x: f32, y: f32| x * share + y * (1. - share);
    gpui_kit::Rgba {
        r: mix(a.r, b.r),
        g: mix(a.g, b.g),
        b: mix(a.b, b.b),
        a: mix(a.a, b.a),
    }
    .into()
}

/// The ghost bars: `muted-foreground/15`.
pub fn ghost_bar(colors: &Colors) -> Hsla {
    colors.muted_foreground.opacity(0.15)
}
