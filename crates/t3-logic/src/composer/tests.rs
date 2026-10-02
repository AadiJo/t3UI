//! Failure modes this covers, written before the tests:
//!
//! 1. Chips: a mention or skill becomes a chip only when whitespace follows; skills take any
//!    currency sigil and may start with a digit but amounts (`$20`, `$20k`, `$1e6`) stay prose;
//!    a bare `@scope/package` stays text; a file link whose label is not the path's basename, or
//!    whose path has a URI scheme, stays text; `@"quoted path"` keeps escapes out of the path;
//!    adjacent chips separated by one space are both found; byte ranges stay correct after
//!    multi-byte text. Context links (`t3-context://v1/<kind>/<id>`, `!` for images) and
//!    citations chip anywhere, win over overlapping mentions, and reject bad kinds, ids, or
//!    citation query keys; context labels are sanitized.
//! 2. Serialization: a path with spaces, parentheses, `#`, `?`, brackets or non-ASCII round-trips
//!    through `serialize_file_link` and `collect_inline_tokens` unchanged.
//! 3. Triggers: `/cmd` only at a line start and without whitespace; currency sigils, `@` and `#`
//!    only for the token under the cursor; U+FFFC no longer breaks a token; a cursor inside a
//!    multi-byte char does not panic.
//! 4. Menu: an empty `/` query lists `/model`, provider commands (only at the prompt start), then
//!    `/skill:` rows, ungrouped; a query ranks by name before description; skills hide disabled
//!    or agent-only ones and show the install source; a selection reuses one following space; the
//!    highlight resets when the query changes and ↑/↓ wrap.
//! 5. Models: the draft's pick beats session, thread, and project; a disabled instance is skipped;
//!    a model typed by name or alias resolves to its slug; an unknown model falls back to the first
//!    model; options carry over only for the same instance.
//! 6. Traits: reasoning `low` reads "Light", the default option is current when nothing is picked,
//!    a pick overrides it, and booleans read "Fast"/"Normal" or "{Label} On/Off".
//! 7. Pending answers: custom text beats the selection and clears it; single-select replaces,
//!    multi-select toggles; nothing is complete until every question has an answer; the panel
//!    summary falls back to file-change for kinds the fork does not name.
//! 8. Send: context chips alone are not sendable text; image-only messages get the fixed text;
//!    titles truncate at 50 chars; attachment limits and the 120,000-char prompt limit report
//!    the reference copy.
//! 9. Drafts: an unreadable or old file starts empty; empty drafts are not written.

use std::collections::BTreeMap;

use serde_json::json;
use t3_protocol::{
    ProviderInstanceId,
    orchestration::{ModelSelection, ProviderOptionSelection, ProviderOptionValue},
    projects::{EntryKind, ProjectEntry},
    server::{ProviderOptionDescriptor, ServerProvider},
};

use super::{
    draft::{ComposerDraft, DraftsFile},
    menu::{self, MenuAction, MenuSources, Replacement},
    pending::{self, DraftAnswer},
    prompt::{self, InlineTokenKind, TriggerKind},
    providers::{self, ModelContext, PickerRail},
    search::SkillSource,
    send,
};
use crate::settings::ModelFavorite;

// ---------------------------------------------------------------------------------------------
// 1-2. Chips

fn kinds(text: &str) -> Vec<(InlineTokenKind, &str)> {
    prompt::collect_inline_tokens(text)
        .into_iter()
        .map(|token| (token.kind, &text[token.range]))
        .collect()
}

fn mention(path: &str) -> InlineTokenKind {
    InlineTokenKind::Mention { path: path.into() }
}

fn skill(name: &str) -> InlineTokenKind {
    InlineTokenKind::Skill { name: name.into() }
}

#[test]
fn chips_need_trailing_whitespace() {
    assert!(kinds("use $review").is_empty());
    assert_eq!(kinds("use $review "), vec![(skill("review"), "$review")]);
    assert_eq!(
        kinds("$9x "),
        vec![(skill("9x"), "$9x")],
        "digits first is fine with a letter"
    );
    for amount in ["$20 ", "$20k ", "$100M ", "$1e6 ", "$1_000 "] {
        assert!(kinds(amount).is_empty(), "{amount:?} is an amount");
    }
    assert_eq!(
        kinds("€review "),
        vec![(skill("review"), "€review")],
        "any currency sigil"
    );
    assert_eq!(
        kinds("@README.md\n"),
        vec![(mention("README.md"), "@README.md")]
    );
    assert!(
        kinds("@scope/pkg ").is_empty(),
        "bare scoped packages stay text"
    );
    assert_eq!(
        kinds("@Src/Main.rs "),
        vec![(mention("Src/Main.rs"), "@Src/Main.rs")]
    );
    assert!(kinds("a@b.c ").is_empty(), "@ must start a token");
}

#[test]
fn file_links_require_matching_basename_and_no_scheme() {
    assert_eq!(
        kinds("[main.rs](src/main.rs) "),
        vec![(mention("src/main.rs"), "[main.rs](src/main.rs)")]
    );
    assert!(kinds("[other](src/main.rs) ").is_empty());
    assert!(kinds("[x](https://x) ").is_empty());
    assert_eq!(
        kinds("[file.txt](C:\\dir\\file.txt) "),
        vec![(
            mention("C:\\dir\\file.txt"),
            "[file.txt](C:\\dir\\file.txt)"
        )]
    );
}

#[test]
fn quoted_mentions_unescape() {
    assert_eq!(
        kinds(r#"@"my dir/a \"b\".md" "#),
        vec![(mention(r#"my dir/a "b".md"#), r#"@"my dir/a \"b\".md""#)]
    );
}

#[test]
fn adjacent_chips_and_multibyte_offsets() {
    let text = "héllo $a $b [c.rs](c.rs) ";
    let found = kinds(text);
    assert_eq!(
        found,
        vec![
            (skill("a"), "$a"),
            (skill("b"), "$b"),
            (mention("c.rs"), "[c.rs](c.rs)"),
        ]
    );
}

#[test]
fn context_links_and_citations() {
    let reference =
        prompt::serialize_context_reference("terminal", "ctx_1", "Terminal 1 [lines]\n4-6");
    assert_eq!(
        reference,
        "[Terminal 1 lines 4-6](t3-context://v1/terminal/ctx_1)"
    );
    let image = prompt::serialize_context_reference("image", "img-2", "");
    assert_eq!(image, "![image](t3-context://v1/image/img-2)");
    // No whitespace needed around links; they beat an overlapping mention.
    let text = format!("see{reference}and {image}@x");
    let found = prompt::collect_inline_tokens(&text);
    assert_eq!(found.len(), 2);
    assert_eq!(&text[found[0].range.clone()], reference);
    assert_eq!(&text[found[1].range.clone()], image);
    assert!(
        matches!(&found[1].kind, InlineTokenKind::Context { image: true, kind, .. } if kind == "image")
    );
    assert!(prompt::collect_inline_tokens("[x](t3-context://v1/Bad/ctx)").is_empty());
    assert!(prompt::collect_inline_tokens("[x](t3-context://v1/terminal/a/b)").is_empty());

    let citation = "[Assistant quote](t3-citation://v1/env/thread/msg?text=Hello+world&start=0&end=11&prefix=&suffix=%21&comment=why)";
    let found = prompt::collect_inline_tokens(citation);
    assert_eq!(
        found[0].kind,
        InlineTokenKind::Citation {
            text: "Hello world".into(),
            comment: Some("why".into())
        }
    );
    let missing_key =
        "[Assistant quote](t3-citation://v1/env/thread/msg?text=a&start=0&end=1&prefix=)";
    assert!(prompt::collect_inline_tokens(missing_key).is_empty());
}

#[test]
fn file_links_round_trip() {
    for path in [
        "src/main.rs",
        "docs/my notes (draft)#1?.md",
        "dir/[weird].txt",
        "ünïcode/файл.md",
        "a/b\\c.txt",
        "100%/x.md",
    ] {
        let link = prompt::serialize_file_link(path);
        let text = format!("see {link} ");
        assert_eq!(kinds(&text), vec![(mention(path), link.as_str())], "{path}");
    }
}

// ---------------------------------------------------------------------------------------------
// 3. Triggers

#[test]
fn slash_only_at_line_start() {
    let trigger = prompt::detect_trigger("/mod", 4).unwrap();
    assert_eq!(trigger.kind, TriggerKind::SlashCommand);
    assert_eq!(trigger.query, "mod");
    assert_eq!(trigger.range, 0..4);
    let second_line = prompt::detect_trigger("hi\n/", 4).unwrap();
    assert_eq!(second_line.range, 3..4);
    assert_eq!(prompt::detect_trigger("/model x", 8), None);
    assert_eq!(prompt::detect_trigger("a /x", 4), None);
}

#[test]
fn skill_and_path_triggers_follow_the_cursor_token() {
    let skill = prompt::detect_trigger("run $cha", 8).unwrap();
    assert_eq!(
        (skill.kind, skill.query.as_str(), skill.range),
        (TriggerKind::Skill, "cha", 4..8)
    );
    let path = prompt::detect_trigger("open @src/ma now", 12).unwrap();
    assert_eq!(
        (path.kind, path.query.as_str()),
        (TriggerKind::Path, "src/ma")
    );
    assert_eq!(prompt::detect_trigger("open @src now", 13), None);
    assert_eq!(
        prompt::detect_trigger("x\u{FFFC}@a", 6),
        None,
        "U+FFFC no longer breaks tokens"
    );
    let pull_request = prompt::detect_trigger("fix #12", 7).unwrap();
    assert_eq!(
        (pull_request.kind, pull_request.query.as_str()),
        (TriggerKind::PullRequest, "12")
    );
    assert_eq!(
        prompt::detect_trigger("#", 1).unwrap().kind,
        TriggerKind::PullRequest
    );
    assert_eq!(prompt::detect_trigger("#-x", 3), None);
    assert_eq!(
        prompt::detect_trigger("£sk", 4).unwrap().kind,
        TriggerKind::Skill
    );
    // Inside a multi-byte char: snapped, no panic.
    assert_eq!(prompt::detect_trigger("é", 1), None);
}

// ---------------------------------------------------------------------------------------------
// 4. Menu

fn config() -> t3_protocol::server::ServerConfig {
    serde_json::from_str(include_str!(
        "../../../t3-snapshots/fixtures/server-config.json"
    ))
    .expect("server config fixture")
}

fn codex(config: &t3_protocol::server::ServerConfig) -> &ServerProvider {
    config
        .providers
        .iter()
        .find(|provider| provider.instance_id.as_str() == "codex")
        .unwrap()
}

fn sources<'a>(
    provider: Option<&'a ServerProvider>,
    entries: &'a [ProjectEntry],
) -> MenuSources<'a> {
    MenuSources {
        instance: provider.map_or("", |provider| provider.instance_id.as_str()),
        slash_commands: provider.map_or(&[][..], |provider| &provider.slash_commands[..]),
        skills: provider.map_or(&[][..], |provider| &provider.skills[..]),
        entries,
        plan_mode: false,
        skills_in_slash_menu: true,
        compact_available: true,
    }
}

#[test]
fn slash_menu_lists_builtin_then_provider() {
    let config = config();
    let trigger = prompt::detect_trigger("/", 1).unwrap();
    let items = menu::menu_items(&trigger, sources(Some(codex(&config)), &[]));
    assert_eq!(items[0].label, "/model");
    assert_eq!(items[0].action, MenuAction::OpenModelPicker);
    assert!(items.iter().any(|item| item.label == "/compact"));
    assert_eq!(
        items.last().unwrap().label,
        "Changelog",
        "skills follow commands"
    );
    // Provider commands only apply at the start of the prompt.
    let later = prompt::detect_trigger("hi\n/", 4).unwrap();
    let items = menu::menu_items(&later, sources(Some(codex(&config)), &[]));
    assert!(!items.iter().any(|item| item.label == "/compact"));

    let typed = prompt::detect_trigger("/comp", 5).unwrap();
    let items = menu::menu_items(&typed, sources(Some(codex(&config)), &[]));
    assert_eq!(items[0].label, "/compact");
    assert_eq!(
        items[0].replacement(),
        Some(Replacement::Text("/compact ".into()))
    );
}

#[test]
fn skill_menu_shows_enabled_skills_with_source() {
    let config = config();
    let trigger = prompt::detect_trigger("$", 1).unwrap();
    let items = menu::menu_items(&trigger, sources(Some(codex(&config)), &[]));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "Changelog");
    assert_eq!(items[0].source.map(SkillSource::label), Some("Personal"));
    assert_eq!(
        items[0].replacement(),
        Some(Replacement::Chip {
            token: "$changelog".into()
        })
    );
    let none = prompt::detect_trigger("$zzzz", 5).unwrap();
    assert!(menu::menu_items(&none, sources(Some(codex(&config)), &[])).is_empty());
    assert_eq!(
        menu::empty_text(TriggerKind::Skill, false),
        "No skills found. Try / to browse provider commands."
    );
}

#[test]
fn path_items_and_selection_range() {
    let trigger = prompt::detect_trigger("see @for", 8).unwrap();
    let entries = vec![ProjectEntry {
        path: "src/format.ts".into(),
        kind: EntryKind::File,
        ignored: None,
    }];
    let items = menu::menu_items(&trigger, sources(None, &entries));
    assert_eq!(items[0].label, "format.ts");
    assert_eq!(items[0].description, "src");
    assert_eq!(items[0].id, "path:file:src/format.ts");
    let text = "see @for rest";
    assert_eq!(prompt::replacement_end(text, 8, "[x](x) "), 9);
    assert_eq!(prompt::replacement_end(text, 8, "/x"), 8);
}

#[test]
fn highlight_resets_and_wraps() {
    let config = config();
    let trigger = prompt::detect_trigger("/", 1).unwrap();
    let items = menu::menu_items(&trigger, sources(Some(codex(&config)), &[]));
    let last = items.last().unwrap().id.clone();
    assert_eq!(
        menu::active_item(&items, Some(&last), true).unwrap().id,
        last
    );
    assert_eq!(
        menu::active_item(&items, Some(&last), false).unwrap().id,
        items[0].id
    );
    assert_eq!(
        menu::nudge(&items, Some(&last), true).unwrap().id,
        items[0].id
    );
    assert_eq!(
        menu::nudge(&items, Some(&items[0].id), false).unwrap().id,
        last
    );
    assert_eq!(menu::nudge(&items, None, true).unwrap().id, items[0].id);
}

// ---------------------------------------------------------------------------------------------
// 5-6. Models and traits

fn selection(instance: &str, model: &str) -> ModelSelection {
    ModelSelection {
        instance_id: ProviderInstanceId::from(instance),
        model: model.into(),
        options: Vec::new(),
    }
}

#[test]
fn model_resolution_order() {
    let config = config();
    let providers = &config.providers;
    let thread = selection("codex", "gpt-6-astra");
    let resolved = providers::resolve_model_selection(
        providers,
        &ModelContext {
            thread_selection: Some(&thread),
            ..Default::default()
        },
    );
    assert_eq!(resolved.instance_id.as_str(), "codex");
    assert_eq!(resolved.model, "gpt-6-astra");

    // Claude is disabled in the fixture: the draft's pick is skipped.
    let claude = ProviderInstanceId::from("claudeAgent");
    let draft = vec![selection("codex", "GPT-6-Luna")];
    let resolved = providers::resolve_model_selection(
        providers,
        &ModelContext {
            draft_active_instance: Some(&claude),
            draft_selections: &draft,
            thread_selection: Some(&thread),
            ..Default::default()
        },
    );
    assert_eq!(resolved.instance_id.as_str(), "codex");
    assert_eq!(resolved.model, "gpt-6-luna", "names resolve to slugs");

    let unknown = selection("codex", "gpt-0");
    let resolved = providers::resolve_model_selection(
        providers,
        &ModelContext {
            thread_selection: Some(&unknown),
            ..Default::default()
        },
    );
    assert_eq!(resolved.model, "gpt-5.4", "falls back to the first model");

    let foreign = ModelSelection {
        options: vec![ProviderOptionSelection {
            id: "effort".into(),
            value: ProviderOptionValue::String("high".into()),
        }],
        ..selection("claudeAgent", "gpt-5.4")
    };
    let resolved = providers::resolve_model_selection(
        providers,
        &ModelContext {
            thread_selection: Some(&foreign),
            ..Default::default()
        },
    );
    assert!(
        resolved.options.is_empty(),
        "options only carry over within an instance"
    );
}

#[test]
fn traits_labels_and_picks() {
    let config = config();
    let model = providers::resolve_model(&codex(&config).models, "gpt-5.4");
    let descriptors = providers::option_descriptors(model, &[]);
    let label: Vec<String> = providers::traits_label(&descriptors, false)
        .into_iter()
        .map(|part| part.label)
        .collect();
    assert_eq!(label, vec!["Medium", "Standard"]);

    let picks = vec![ProviderOptionSelection {
        id: "reasoningEffort".into(),
        value: ProviderOptionValue::String("low".into()),
    }];
    let descriptors = providers::option_descriptors(model, &picks);
    assert_eq!(
        providers::traits_label(&descriptors, false)[0].label,
        "Light"
    );
    let dispatch = providers::selections_from_descriptors(&descriptors);
    assert_eq!(dispatch.len(), 2);
    assert_eq!(
        dispatch[1].value,
        ProviderOptionValue::String("default".into())
    );

    let fast = vec![ProviderOptionDescriptor::Boolean {
        id: "fastMode".into(),
        label: "Fast".into(),
        description: None,
        current_value: Some(true),
    }];
    assert_eq!(providers::traits_label(&fast, false)[0].label, "Fast");
    let updated =
        providers::with_descriptor_value(&fast, "fastMode", &ProviderOptionValue::Bool(false));
    assert_eq!(providers::traits_label(&updated, false)[0].label, "Normal");
}

#[test]
fn picker_lists_ready_instances_and_favorites() {
    let config = config();
    let providers = &config.providers;
    let codex_id = ProviderInstanceId::from("codex");
    let rows =
        providers::picker_models(providers, &[], &PickerRail::Instance(codex_id.clone()), "");
    assert_eq!(
        rows.iter()
            .map(|row| row.model.slug.as_str())
            .collect::<Vec<_>>(),
        vec!["gpt-5.4", "gpt-5.4-mini", "gpt-6-astra", "gpt-6-luna"]
    );
    let favorites = vec![ModelFavorite {
        provider: "codex".into(),
        model: "gpt-6-luna".into(),
    }];
    let rows = providers::picker_models(
        providers,
        &favorites,
        &PickerRail::Instance(codex_id.clone()),
        "",
    );
    assert_eq!(rows[0].model.slug, "gpt-6-luna");
    let rows = providers::picker_models(providers, &favorites, &PickerRail::Favorites, "");
    assert_eq!(rows.len(), 1);
    // Disabled Claude never contributes, even to a search.
    let rows = providers::picker_models(providers, &[], &PickerRail::Favorites, "opus");
    assert!(rows.is_empty());
    let rows = providers::picker_models(providers, &[], &PickerRail::Favorites, "mini");
    assert_eq!(rows[0].model.slug, "gpt-5.4-mini");
    assert_eq!(
        providers::initial_rail(&favorites, &codex_id),
        PickerRail::Favorites
    );
}

#[test]
fn display_names() {
    let config = config();
    let entries = providers::provider_entries(&config.providers);
    assert_eq!(entries[0].display_name(), "Codex");
    assert_eq!(providers::instance_initials("Codex Personal"), "CP");
    assert_eq!(providers::instance_initials("codex"), "CO");
    assert_eq!(providers::driver_label("myDriver"), "My Driver");
}

// ---------------------------------------------------------------------------------------------
// 7. Pending requests

#[test]
fn user_input_answers() {
    let questions: [pending::QuestionShape<'_>; 1] = [("db", false)];
    let mut drafts = BTreeMap::new();
    let progress = pending::progress(&questions, &drafts, 0);
    assert!(!progress.can_advance && progress.is_last && !progress.is_complete);

    let mut answer = DraftAnswer::default();
    pending::toggle_option(false, &mut answer, "SQLite");
    drafts.insert("db".to_owned(), answer.clone());
    assert_eq!(
        pending::build_answers(&questions, &drafts).unwrap()["db"],
        json!("SQLite")
    );
    pending::set_custom_answer(&mut answer, "  DuckDB ");
    assert!(answer.selected.is_empty());
    drafts.insert("db".to_owned(), answer);
    assert_eq!(
        pending::build_answers(&questions, &drafts).unwrap()["db"],
        json!("DuckDB")
    );

    let mut multi = DraftAnswer::default();
    pending::toggle_option(true, &mut multi, "a");
    pending::toggle_option(true, &mut multi, "b");
    pending::toggle_option(true, &mut multi, "a");
    assert_eq!(
        pending::resolve_answer(true, Some(&multi)),
        Some(json!(["b"]))
    );

    assert_eq!(
        pending::primary_action_label(false, true, false, 0),
        "Submit answer"
    );
    assert_eq!(
        pending::primary_action_label(false, true, false, 1),
        "Submit answers"
    );
    assert_eq!(
        pending::primary_action_label(true, true, false, 0),
        "Submit"
    );
    assert_eq!(
        pending::approval_summary("file-read"),
        "File-read approval requested"
    );
    assert_eq!(
        pending::approval_summary("permission"),
        "File-change approval requested"
    );
}

#[test]
fn token_formatting() {
    assert_eq!(pending::format_tokens(950.0), "950");
    assert_eq!(pending::format_tokens(1_200.0), "1.2k");
    assert_eq!(pending::format_tokens(2_000.0), "2k");
    assert_eq!(pending::format_tokens(12_345.0), "12k");
    assert_eq!(pending::format_tokens(1_250_000.0), "1.3m");
}

// ---------------------------------------------------------------------------------------------
// 8. Send

#[test]
fn send_rules() {
    let terminal_chip = " [Terminal 1 line 4](t3-context://v1/terminal/ctx_1) ";
    assert!(
        !send::has_sendable_content(terminal_chip, 0, 0, 0),
        "a chip alone is not text"
    );
    assert!(send::has_sendable_content(terminal_chip, 0, 1, 0));
    assert!(send::has_sendable_content("", 1, 0, 0));
    assert!(send::has_sendable_content("", 0, 0, 1));
    assert_eq!(
        send::prompt_length_error(&"x".repeat(send::MAX_INPUT_CHARS)),
        None
    );
    assert_eq!(
        send::prompt_length_error(&"x".repeat(send::MAX_INPUT_CHARS + 1234)).as_deref(),
        Some(
            "Prompt is 1,234 characters over the 120,000-character limit. Shorten or split it before sending."
        )
    );
    assert_eq!(send::outgoing_text("  ", 1), send::IMAGE_ONLY_TEXT);
    assert_eq!(send::title_seed("", Some("shot.png")), "Image: shot.png");
    assert_eq!(send::title_seed("", None), "New thread");
    let long = "x".repeat(60);
    assert_eq!(
        send::title_seed(&long, None),
        format!("{}...", "x".repeat(50))
    );
    assert_eq!(
        send::check_attachment("a.txt", "text/plain", 1, 0)
            .unwrap_err()
            .to_string(),
        "Unsupported file type for 'a.txt'. Please attach image files only."
    );
    assert_eq!(
        send::check_attachment("a.png", "image/png", 11 * 1024 * 1024, 0)
            .unwrap_err()
            .to_string(),
        "'a.png' exceeds the 10MB attachment limit."
    );
    assert_eq!(
        send::check_attachment("a.png", "image/png", 1, 8)
            .unwrap_err()
            .to_string(),
        "You can attach up to 8 images per message."
    );
}

// ---------------------------------------------------------------------------------------------
// 9. Drafts

#[test]
fn drafts_file_round_trip() {
    assert_eq!(DraftsFile::from_json("not json"), DraftsFile::default());
    assert_eq!(
        DraftsFile::from_json(r#"{"version": 99, "draftsByThreadKey": {"a": {"prompt": "x"}}}"#),
        DraftsFile::default()
    );
    let mut file = DraftsFile::default();
    file.drafts_by_thread_key.insert(
        "env:thread".into(),
        ComposerDraft {
            prompt: "hello $changelog ".into(),
            ..Default::default()
        },
    );
    file.drafts_by_thread_key
        .insert("empty".into(), ComposerDraft::default());
    let decoded = DraftsFile::from_json(&file.to_json());
    assert_eq!(decoded.drafts_by_thread_key.len(), 1);
    assert_eq!(
        decoded.drafts_by_thread_key["env:thread"].prompt,
        "hello $changelog "
    );
}
