//! Token sheet: type scale, color tokens, status colors, icons, logos and file icons.

use gpui_kit::{
    AnyElement, App, FontWeight, Hsla, IntoElement, ParentElement, Pixels, Styled, div, px,
};
use t3_ui::{Colors, Icon, IconName, Logo, file_icon, logo, tokens};

use super::section;

pub(super) fn tokens_page(colors: &'static Colors, cx: &App) -> Vec<AnyElement> {
    let type_rows = [
        ("text-xs", tokens::text::XS),
        ("text-sm", tokens::text::SM),
        ("text-base", tokens::text::BASE),
        ("text-lg", tokens::text::LG),
        ("text-xl", tokens::text::XL),
        ("text-2xl", tokens::text::XL2),
    ];
    let weights = [
        FontWeight::NORMAL,
        FontWeight::MEDIUM,
        FontWeight::SEMIBOLD,
        FontWeight::BOLD,
    ];
    let typography = div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .children(type_rows.map(|(name, (size, line))| {
            div()
                .flex()
                .items_center()
                .gap(px(24.))
                .child(type_label(name, size, line, colors))
                .children(weights.map(|weight| {
                    div()
                        .w(px(300.))
                        .text_size(size)
                        .line_height(line)
                        .font_weight(weight)
                        .child("The quick brown fox 0123")
                }))
        }))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(24.))
                .child(type_label("font-mono", px(12.), px(16.), colors))
                .children(
                    [
                        (
                            FontWeight::NORMAL,
                            false,
                            "fn main() { println!(\"{}\", 0x1F_u8); } // il1| O0",
                        ),
                        (FontWeight::MEDIUM, false, "medium 500"),
                        (FontWeight::BOLD, false, "bold 700"),
                        (FontWeight::NORMAL, true, "italic 400"),
                        (FontWeight::BOLD, true, "bold italic 700"),
                    ]
                    .map(|(weight, italic, text)| {
                        let sample = div()
                            .font_family(t3_ui::Theme::global(cx).mono_family().clone())
                            .text_size(px(12.))
                            .line_height(px(16.))
                            .font_weight(weight)
                            .child(text);
                        if italic { sample.italic() } else { sample }
                    }),
                ),
        );

    let core: [(&str, Hsla); 26] = [
        ("background", colors.background),
        ("foreground", colors.foreground),
        ("card", colors.card),
        ("popover", colors.popover),
        ("primary", colors.primary),
        ("primary-fg", colors.primary_foreground),
        ("secondary", colors.secondary),
        ("muted", colors.muted),
        ("muted-fg", colors.muted_foreground),
        ("accent", colors.accent),
        ("border", colors.border),
        ("input", colors.input),
        ("ring", colors.ring),
        ("destructive", colors.destructive),
        ("destructive-fg", colors.destructive_foreground),
        ("info", colors.info),
        ("info-fg", colors.info_foreground),
        ("success", colors.success),
        ("success-fg", colors.success_foreground),
        ("warning", colors.warning),
        ("warning-fg", colors.warning_foreground),
        ("sidebar-glass", colors.app_sidebar_glass),
        ("main-glass", colors.app_main_glass),
        ("ring/24", colors.ring_24),
        ("input/32", colors.input_32),
        ("accent/50", colors.accent_50),
    ];
    let status = colors.status;
    let status_rows = [
        ("working", status.working),
        ("pending", status.pending_approval),
        ("awaiting", status.awaiting_input),
        ("error", status.error),
        ("plan-ready", status.plan_ready),
        ("completed", status.completed),
        ("pr-closed", status.pr_closed),
        ("pr-merged", status.pr_merged),
        ("terminal", status.terminal_running),
    ];

    let file_names = [
        "main.rs",
        "index.tsx",
        "app.ts",
        "styles.css",
        "README.md",
        "package.json",
        "tsconfig.json",
        "AGENTS.md",
        "CLAUDE.md",
        "pnpm-lock.yaml",
        "Dockerfile",
        "data.json",
        "photo.png",
        "notes.txt",
        "unknown.xyz",
    ];

    vec![
        section(
            "Typography (DM Sans 400 / 500 / 600 / 700)",
            colors,
            typography,
        ),
        section(
            "Color tokens",
            colors,
            div()
                .flex()
                .flex_wrap()
                .gap(px(12.))
                .children(core.map(|(name, color)| swatch(name, color, colors))),
        ),
        section(
            "Status",
            colors,
            div()
                .flex()
                .gap(px(20.))
                .children(status_rows.map(|(name, color)| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .text_color(color.text)
                        .child(div().size(px(9.)).rounded_full().bg(color.dot))
                        .child(name)
                })),
        ),
        section(
            "Lucide icons (16px, foreground / muted)",
            colors,
            div()
                .flex()
                .flex_wrap()
                .gap(px(14.))
                .children(IconName::ALL.iter().enumerate().map(|(ix, name)| {
                    Icon::new(*name).color(if ix % 2 == 0 {
                        colors.foreground
                    } else {
                        colors.muted_foreground
                    })
                })),
        ),
        section(
            "Logos (20px)",
            colors,
            div().flex().flex_wrap().gap(px(14.)).children(
                Logo::ALL
                    .iter()
                    .map(|name| logo(*name, colors.is_dark, px(20.))),
            ),
        ),
        section(
            "File icons (16px)",
            colors,
            div()
                .flex()
                .flex_wrap()
                .gap(px(16.))
                .children(file_names.map(|name| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .child(file_icon(name, colors.is_dark).render(px(16.)))
                        .child(name)
                })),
        ),
    ]
}

fn type_label(name: &str, size: Pixels, line: Pixels, colors: &Colors) -> impl IntoElement {
    div()
        .w(px(130.))
        .text_size(px(11.))
        .line_height(px(14.))
        .text_color(colors.muted_foreground)
        .child(format!("{name} {}/{}", f32::from(size), f32::from(line)))
}

fn swatch(name: &str, color: Hsla, colors: &Colors) -> impl IntoElement {
    let rgba = color.to_rgb();
    let hex = format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        (rgba.r * 255.).round() as u8,
        (rgba.g * 255.).round() as u8,
        (rgba.b * 255.).round() as u8,
        (rgba.a * 255.).round() as u8
    );
    div()
        .w(px(120.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .h(px(40.))
                .rounded(px(8.))
                .border_1()
                .border_color(colors.border)
                .bg(color),
        )
        .child(
            div()
                .text_size(px(11.))
                .line_height(px(14.))
                .child(name.to_string()),
        )
        .child(
            div()
                .text_size(px(10.))
                .line_height(px(12.))
                .text_color(colors.muted_foreground)
                .child(hex),
        )
}
