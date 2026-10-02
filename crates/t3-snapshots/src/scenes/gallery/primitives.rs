//! Primitive pages: buttons and badges, form controls, and overlay surfaces.

use gpui_kit::{AnyElement, IntoElement, ParentElement as _, Styled as _, div, px};
use t3_ui::{
    Badge, BadgeSize, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, CardFooter,
    CardHeader, CardPanel, Checkbox, CheckedState, Colors, DialogDescription, DialogFooter,
    DialogHeader, DialogPanel, DialogPopup, DialogTitle, FooterVariant, IconName, Input, InputSize,
    Interaction, Kbd, MenuCheckboxItem, MenuGroupLabel, MenuItem, MenuPopup, MenuSeparator,
    PopoverDescription, PopoverPopup, PopoverTitle, SelectItem, SelectPopup, SelectSize,
    SelectTrigger, SelectVariant, Separator, SheetPopup, Skeleton, Spinner, Switch, Textarea,
    Toast, ToastKind, TooltipPopup, dialog_backdrop,
};

use super::{Fields, row, section};

const STATES: [(&str, Interaction); 4] = [
    ("rest", Interaction::Rest),
    ("hover", Interaction::Hover),
    ("pressed", Interaction::Pressed),
    ("focus-visible", Interaction::FocusVisible),
];

pub(super) fn buttons_page(colors: &'static Colors) -> Vec<AnyElement> {
    let variants = [
        ("default", ButtonVariant::Default),
        ("destructive", ButtonVariant::Destructive),
        ("destructive-outline", ButtonVariant::DestructiveOutline),
        ("outline", ButtonVariant::Outline),
        ("secondary", ButtonVariant::Secondary),
        ("ghost", ButtonVariant::Ghost),
        ("link", ButtonVariant::Link),
    ];
    let variant_rows =
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .children(variants.map(|(name, variant)| {
                let mut items: Vec<AnyElement> = STATES
                    .iter()
                    .map(|(state, interaction)| {
                        Button::new(format!("{name}-{state}"))
                            .variant(variant)
                            .icon(IconName::Plus)
                            .label(capitalize(state))
                            .preview(*interaction)
                            .into_any_element()
                    })
                    .collect();
                items.push(
                    Button::new(format!("{name}-disabled"))
                        .variant(variant)
                        .icon(IconName::Plus)
                        .label("Disabled")
                        .disabled(true)
                        .into_any_element(),
                );
                row(name, colors, items)
            }));

    let sizes = [
        ("xs", ButtonSize::Xs),
        ("sm", ButtonSize::Sm),
        ("default", ButtonSize::Default),
        ("lg", ButtonSize::Lg),
        ("xl", ButtonSize::Xl),
    ];
    let icon_sizes = [
        ("icon-xs", ButtonSize::IconXs),
        ("icon-sm", ButtonSize::IconSm),
        ("icon", ButtonSize::Icon),
        ("icon-lg", ButtonSize::IconLg),
        ("icon-xl", ButtonSize::IconXl),
    ];
    let size_rows = div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(row(
            "sizes (default)",
            colors,
            sizes.map(|(name, size)| {
                Button::new(format!("size-{name}"))
                    .size(size)
                    .icon(IconName::Download)
                    .label(name)
                    .into_any_element()
            }),
        ))
        .child(row(
            "sizes (outline)",
            colors,
            sizes.map(|(name, size)| {
                Button::new(format!("size-outline-{name}"))
                    .variant(ButtonVariant::Outline)
                    .size(size)
                    .label(name)
                    .icon_end(IconName::ChevronDown)
                    .into_any_element()
            }),
        ))
        .child(row(
            "icon sizes",
            colors,
            icon_sizes
                .iter()
                .map(|(name, size)| {
                    Button::new(format!("icon-{name}"))
                        .variant(ButtonVariant::Outline)
                        .size(*size)
                        .icon(IconName::Settings)
                        .into_any_element()
                })
                .chain(icon_sizes.iter().map(|(name, size)| {
                    Button::new(format!("ghost-{name}"))
                        .variant(ButtonVariant::Ghost)
                        .size(*size)
                        .icon(IconName::PanelLeftClose)
                        .into_any_element()
                })),
        ))
        .child(row(
            "titlebar toggles",
            colors,
            [
                Button::new("toggle-rest")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .child(t3_ui::Icon::new(IconName::PanelLeftClose).size(px(14.)))
                    .into_any_element(),
                Button::new("toggle-pressed")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .pressed(true)
                    .child(t3_ui::Icon::new(IconName::SquareTerminal).size(px(14.)))
                    .into_any_element(),
            ],
        ));

    let badge_variants = [
        ("default", BadgeVariant::Default),
        ("secondary", BadgeVariant::Secondary),
        ("outline", BadgeVariant::Outline),
        ("destructive", BadgeVariant::Destructive),
        ("error", BadgeVariant::Error),
        ("info", BadgeVariant::Info),
        ("success", BadgeVariant::Success),
        ("warning", BadgeVariant::Warning),
    ];
    let badges = div().flex().flex_col().gap(px(12.)).children(
        [
            ("sm", BadgeSize::Sm),
            ("default", BadgeSize::Default),
            ("lg", BadgeSize::Lg),
        ]
        .map(|(size_name, size)| {
            row(
                &format!("badge {size_name}"),
                colors,
                badge_variants.map(|(name, variant)| {
                    Badge::new(name)
                        .variant(variant)
                        .size(size)
                        .when_icon(name == "info", IconName::Info)
                        .into_any_element()
                }),
            )
        }),
    );
    let kbds = row(
        "kbd",
        colors,
        [
            div()
                .flex()
                .gap(px(4.))
                .child(Kbd::new("⌘"))
                .child(Kbd::new("K"))
                .into_any_element(),
            Kbd::new("Esc").into_any_element(),
            Kbd::new("⇧⌘P").into_any_element(),
        ],
    );

    vec![
        section("Button variants x states", colors, variant_rows),
        section("Button sizes", colors, size_rows),
        section("Badges", colors, badges),
        section("Kbd", colors, kbds),
    ]
}

trait BadgeExt {
    fn when_icon(self, condition: bool, icon: IconName) -> Self;
}

impl BadgeExt for Badge {
    fn when_icon(self, condition: bool, icon: IconName) -> Self {
        if condition { self.icon(icon) } else { self }
    }
}

pub(super) fn controls_page(colors: &'static Colors, fields: &Fields) -> Vec<AnyElement> {
    let field = |content: AnyElement| div().w(px(220.)).child(content).into_any_element();
    let inputs = div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(row(
            "input states",
            colors,
            [
                field(Input::new(&fields.empty).into_any_element()),
                field(Input::new(&fields.filled).into_any_element()),
                field(
                    Input::new(&fields.filled)
                        .preview(Interaction::FocusVisible)
                        .into_any_element(),
                ),
                field(Input::new(&fields.filled).invalid(true).into_any_element()),
                field(
                    Input::new(&fields.filled)
                        .invalid(true)
                        .preview(Interaction::FocusVisible)
                        .into_any_element(),
                ),
            ],
        ))
        .child(row(
            "input sizes / disabled",
            colors,
            [
                field(
                    Input::new(&fields.empty)
                        .size(InputSize::Sm)
                        .into_any_element(),
                ),
                field(Input::new(&fields.empty).into_any_element()),
                field(
                    Input::new(&fields.empty)
                        .size(InputSize::Lg)
                        .into_any_element(),
                ),
                field(Input::new(&fields.filled).disabled(true).into_any_element()),
            ],
        ))
        .child(row(
            "textarea",
            colors,
            [
                div()
                    .w(px(320.))
                    .child(Textarea::new(&fields.notes))
                    .into_any_element(),
                div()
                    .w(px(320.))
                    .child(Textarea::new(&fields.notes).preview(Interaction::FocusVisible))
                    .into_any_element(),
            ],
        ));

    let select = |id: &str, configure: fn(SelectTrigger) -> SelectTrigger| {
        div()
            .w(px(200.))
            .child(configure(
                SelectTrigger::new(id.to_string()).placeholder("Select a model"),
            ))
            .into_any_element()
    };
    let selects = div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(row(
            "select trigger",
            colors,
            [
                select("s-placeholder", |t| t),
                select("s-value", |t| t.value(Some("GPT-5 Codex".into()))),
                select("s-pressed", |t| {
                    t.value(Some("GPT-5 Codex".into())).pressed(true)
                }),
                select("s-focus", |t| {
                    t.value(Some("GPT-5 Codex".into()))
                        .preview(Interaction::FocusVisible)
                }),
                select("s-invalid", |t| t.invalid(true)),
                select("s-disabled", |t| {
                    t.value(Some("GPT-5 Codex".into())).disabled(true)
                }),
            ],
        ))
        .child(row(
            "select sizes",
            colors,
            [
                select("s-xs", |t| t.size(SelectSize::Xs).value(Some("xs".into()))),
                select("s-sm", |t| t.size(SelectSize::Sm).value(Some("sm".into()))),
                select("s-md", |t| t.value(Some("default".into()))),
                select("s-lg", |t| t.size(SelectSize::Lg).value(Some("lg".into()))),
            ],
        ))
        .child(row(
            "select ghost",
            colors,
            [
                SelectTrigger::new("g-rest")
                    .variant(SelectVariant::Ghost)
                    .size(SelectSize::Xs)
                    .value(Some("Full access".into()))
                    .into_any_element(),
                SelectTrigger::new("g-hover")
                    .variant(SelectVariant::Ghost)
                    .size(SelectSize::Xs)
                    .value(Some("Full access".into()))
                    .preview(Interaction::Hover)
                    .into_any_element(),
                SelectTrigger::new("g-pressed")
                    .variant(SelectVariant::Ghost)
                    .size(SelectSize::Xs)
                    .value(Some("Full access".into()))
                    .pressed(true)
                    .into_any_element(),
            ],
        ));

    let switches = row(
        "switch",
        colors,
        [false, true].into_iter().flat_map(|checked| {
            STATES
                .iter()
                .filter(|(_, interaction)| *interaction != Interaction::Hover)
                .map(move |(state, interaction)| {
                    Switch::new(format!("sw-{checked}-{state}"))
                        .checked(checked)
                        .preview(*interaction)
                        .into_any_element()
                })
                .chain(std::iter::once(
                    Switch::new(format!("sw-{checked}-disabled"))
                        .checked(checked)
                        .disabled(true)
                        .into_any_element(),
                ))
                .chain(std::iter::once(
                    Switch::new(format!("sw-{checked}-small"))
                        .checked(checked)
                        .small()
                        .into_any_element(),
                ))
        }),
    );
    let checkboxes = row(
        "checkbox",
        colors,
        [
            CheckedState::Unchecked,
            CheckedState::Checked,
            CheckedState::Indeterminate,
        ]
        .into_iter()
        .flat_map(|state| {
            [
                Checkbox::new(format!("cb-{state:?}"))
                    .state(state)
                    .into_any_element(),
                Checkbox::new(format!("cb-{state:?}-focus"))
                    .state(state)
                    .preview(Interaction::FocusVisible)
                    .into_any_element(),
                Checkbox::new(format!("cb-{state:?}-disabled"))
                    .state(state)
                    .disabled(true)
                    .into_any_element(),
            ]
        })
        .chain([
            Checkbox::new("cb-invalid").invalid(true).into_any_element(),
            Checkbox::new("cb-label")
                .checked(true)
                .label("Show archived threads")
                .into_any_element(),
        ]),
    );

    let feedback = div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(row(
            "spinner",
            colors,
            [
                Spinner::new("spin-14").size(px(14.)).into_any_element(),
                Spinner::new("spin-16").into_any_element(),
                Spinner::new("spin-20").size(px(20.)).into_any_element(),
            ],
        ))
        .child(row(
            "skeleton",
            colors,
            [div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(Skeleton::new("sk-1").w(px(260.)).h(px(14.)))
                .child(Skeleton::new("sk-2").w(px(200.)).h(px(14.)))
                .child(Skeleton::new("sk-3").still().w(px(320.)).h(px(40.)))
                .into_any_element()],
        ))
        .child(row(
            "separator",
            colors,
            [
                div()
                    .w(px(240.))
                    .child(Separator::horizontal())
                    .into_any_element(),
                div()
                    .h(px(24.))
                    .flex()
                    .child(Separator::vertical())
                    .into_any_element(),
            ],
        ));

    let card = Card::new()
        .w(px(420.))
        .header(
            CardHeader::new()
                .title("Codex")
                .description("OpenAI's coding agent, signed in as aadi"),
        )
        .panel(
            CardPanel::new().child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child("Enable provider")
                    .child(Switch::new("card-switch").checked(true)),
            ),
        )
        .footer(
            CardFooter::new().child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        Button::new("card-cancel")
                            .variant(ButtonVariant::Outline)
                            .label("Cancel"),
                    )
                    .child(Button::new("card-save").label("Save")),
            ),
        );

    vec![
        section("Input / textarea", colors, inputs),
        section("Select", colors, selects),
        section(
            "Switch / checkbox",
            colors,
            div()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(switches)
                .child(checkboxes),
        ),
        section("Spinner / skeleton / separator", colors, feedback),
        section("Card", colors, card),
    ]
}

pub(super) fn overlays_page(colors: &'static Colors) -> Vec<AnyElement> {
    let menu = MenuPopup::new()
        .w(px(240.))
        .child(MenuGroupLabel::new("Thread"))
        .child(
            MenuItem::new("m-rename", "Rename")
                .icon(IconName::SquarePen)
                .shortcut("⌘R"),
        )
        .child(
            MenuItem::new("m-copy", "Copy link")
                .icon(IconName::Link)
                .preview(Interaction::Hover),
        )
        .child(MenuItem::new("m-inset", "Inset item").inset())
        .child(
            MenuItem::new("m-disabled", "Disabled")
                .icon(IconName::Archive)
                .disabled(true),
        )
        .child(MenuSeparator)
        .child(MenuCheckboxItem::new("m-check", "Show diff stats").checked(true))
        .child(MenuCheckboxItem::new("m-check-off", "Wrap long lines"))
        .child(
            MenuCheckboxItem::new("m-switch", "Auto-archive")
                .switch()
                .checked(true),
        )
        .child(MenuSeparator)
        .child(
            MenuItem::new("m-delete", "Delete thread")
                .icon(IconName::Trash2)
                .destructive(),
        );

    // The traits picker's compact menu (`TraitsPicker.tsx`, xs trigger).
    let compact_menu = MenuPopup::new()
        .compact()
        .w(px(160.))
        .child(MenuGroupLabel::new("Reasoning"))
        .children(
            ["Light", "Medium (default)", "High", "Extra High"].map(|label| {
                MenuCheckboxItem::new(format!("c-{label}"), label)
                    .compact()
                    .checked(label == "Medium (default)")
            }),
        )
        .child(MenuSeparator)
        .child(MenuGroupLabel::new("Service Tier"))
        .child(
            MenuCheckboxItem::new("c-standard", "Standard (default)")
                .compact()
                .checked(true),
        )
        .child(MenuCheckboxItem::new("c-fast", "Fast").compact());

    let select_popup = div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            SelectTrigger::new("sp-trigger")
                .value(Some("Medium".into()))
                .pressed(true)
                .w(px(200.)),
        )
        .child(
            SelectPopup::new()
                .min_width(px(200.))
                .child(SelectItem::new("sp-low", "Low"))
                .child(SelectItem::new("sp-med", "Medium").selected(true))
                .child(SelectItem::new("sp-high", "High").preview(Interaction::Hover))
                .child(SelectItem::new("sp-off", "Unavailable").disabled(true)),
        );

    let popovers = div()
        .flex()
        .items_start()
        .gap(px(24.))
        .child(
            PopoverPopup::new()
                .w(px(280.))
                .gap(px(8.))
                .child(PopoverTitle::new("Context window"))
                .child(PopoverDescription::new(
                    "42% of 272k tokens used in this thread.",
                )),
        )
        .child(
            PopoverPopup::new()
                .tooltip_style()
                .child("Tooltip-style popover"),
        )
        .child(TooltipPopup::new("Drag to resize sidebar"))
        .child(TooltipPopup::new("New thread ⌘N"));

    let toasts = div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .w(px(360.))
        .child(Toast::new("Settings saved").id("t-default"))
        .child(
            Toast::new("Pushed to origin")
                .id("t-success")
                .kind(ToastKind::Success)
                .description("feat/design-system is up to date."),
        )
        .child(
            Toast::new("Connection lost")
                .id("t-error")
                .kind(ToastKind::Error)
                .description("Retrying in 5 seconds.")
                .action("Retry", |_, _, _| {}),
        )
        .child(
            Toast::new("Update available")
                .id("t-info")
                .kind(ToastKind::Info),
        )
        .child(
            Toast::new("Uncommitted changes")
                .id("t-warning")
                .kind(ToastKind::Warning),
        )
        .child(
            Toast::new("Cloning repository…")
                .id("t-loading")
                .kind(ToastKind::Loading),
        );

    let dialog =
        div()
            .relative()
            .w(px(640.))
            .h(px(380.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.))
            .overflow_hidden()
            .border_1()
            .border_color(colors.border)
            .child(div().absolute().inset_0().p(px(24.)).child(
                "Window content behind the backdrop. Lorem ipsum dolor sit amet, consectetur.",
            ))
            .child(dialog_backdrop_el())
            .child(
                DialogPopup::new()
                    .close_button(|_, _, _| {})
                    .child(
                        DialogHeader::new()
                            .before_panel()
                            .child(DialogTitle::new("Rename thread"))
                            .child(DialogDescription::new(
                                "Give this thread a name you'll recognize.",
                            )),
                    )
                    .child(DialogPanel::new().after_header().child("Panel content"))
                    .child(
                        DialogFooter::new()
                            .child(
                                Button::new("d-cancel")
                                    .variant(ButtonVariant::Outline)
                                    .label("Cancel"),
                            )
                            .child(Button::new("d-save").label("Save")),
                    ),
            );
    let alert = DialogPopup::new()
        .w(px(420.))
        .child(
            DialogHeader::new()
                .child(DialogTitle::new("Delete thread?"))
                .child(DialogDescription::new(
                    "This permanently removes the thread and its history.",
                )),
        )
        .child(
            DialogFooter::new()
                .variant(FooterVariant::Bare)
                .child(
                    Button::new("a-cancel")
                        .variant(ButtonVariant::Outline)
                        .label("Cancel"),
                )
                .child(
                    Button::new("a-delete")
                        .variant(ButtonVariant::Destructive)
                        .label("Delete"),
                ),
        );
    let sheet = div().h(px(380.)).flex().child(
        SheetPopup::new().child(DialogHeader::new().child(DialogTitle::new("Diff")).child(
            DialogDescription::new("Right panel as a sheet below 980px."),
        )),
    );

    vec![
        section(
            "Menu / select popup",
            colors,
            div()
                .flex()
                .items_start()
                .gap(px(32.))
                .child(menu)
                .child(compact_menu)
                .child(select_popup),
        ),
        section("Popover / tooltip", colors, popovers),
        section(
            "Toasts / dialog / alert dialog / sheet",
            colors,
            div()
                .flex()
                .items_start()
                .gap(px(32.))
                .child(toasts)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(24.))
                        .child(dialog)
                        .child(alert),
                )
                .child(sheet),
        ),
    ]
}

fn dialog_backdrop_el() -> impl IntoElement {
    BackdropHost
}

/// Renders the dialog backdrop with access to the app context.
#[derive(gpui_kit::IntoElement)]
struct BackdropHost;

impl gpui_kit::RenderOnce for BackdropHost {
    fn render(self, _: &mut gpui_kit::Window, cx: &mut gpui_kit::App) -> impl IntoElement {
        dialog_backdrop(cx)
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
