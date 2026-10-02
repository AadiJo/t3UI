//! Failure modes this covers, written before the tests:
//!
//! 1. Chips: a token becomes a chip only when whitespace follows; `$9x` is not a skill; a file
//!    link whose label is not the path's basename, or whose path has a URI scheme, stays text;
//!    `@"quoted path"` keeps escapes out of the path; adjacent chips separated by one space are
//!    both found; byte ranges stay correct after multi-byte text.
//! 2. Serialization: a path with spaces, parentheses, `#`, `?`, brackets or non-ASCII round-trips
//!    through `serialize_file_link` and `collect_inline_tokens` unchanged.
//! 3. Triggers: `/cmd` only at a line start and without whitespace; `$` and `@` only for the token
//!    under the cursor; U+FFFC breaks a token; a cursor inside a multi-byte char does not panic.
//! 4. Menu: an empty `/` query lists `/model` then provider commands, grouped Built-in / Provider;
//!    a query ranks by name before description; skills hide disabled ones and show the install
//!    source; a selection reuses one following space; the highlight resets when the query changes
//!    and ↑/↓ wrap.
//! 5. Models: the draft's pick beats session, thread, and project; a disabled instance is skipped;
//!    a model typed by name or alias resolves to its slug; an unknown model falls back to the first
//!    model; options carry over only for the same instance.
//! 6. Traits: reasoning `low` reads "Light", the default option is current when nothing is picked,
//!    a pick overrides it, and booleans read "Fast"/"Normal" or "{Label} On/Off".
//! 7. Pending answers: custom text beats the selection and clears it; single-select replaces,
//!    multi-select toggles; nothing is complete until every question has an answer; the panel
//!    summary falls back to file-change for kinds the fork does not name.
//! 8. Send: placeholders alone are not sendable; image-only messages get the fixed text; titles
//!    truncate at 50 chars; attachment limits report the reference copy.
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
    menu::{self, MenuAction, Replacement},
    pending::{self, DraftAnswer},
    prompt::{self, InlineTokenKind, TriggerKind},
    providers::{self, ModelContext, PickerRail},
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
    assert!(kinds("$9x ").is_empty());
    assert_eq!(
        kinds("@src/main.rs\n"),
        vec![(mention("src/main.rs"), "@src/main.rs")]
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
    let after_placeholder = prompt::detect_trigger("x\u{FFFC}@a", 6).unwrap();
    assert_eq!(after_placeholder.range, 4..6);
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

#[test]
fn slash_menu_lists_builtin_then_provider() {
    let config = config();
    let trigger = prompt::detect_trigger("/", 1).unwrap();
    let items = menu::menu_items(&trigger, Some(codex(&config)), &[]);
    assert_eq!(items[0].label, "/model");
    assert_eq!(items[0].action, MenuAction::OpenModelPicker);
    assert!(items.iter().any(|item| item.label == "/compact"));
    let groups = menu::group_items(&items, &trigger);
    assert_eq!(
        groups.iter().map(|group| group.label).collect::<Vec<_>>(),
        vec![Some("Built-in"), Some("Provider")]
    );

    let typed = prompt::detect_trigger("/comp", 5).unwrap();
    let items = menu::menu_items(&typed, Some(codex(&config)), &[]);
    assert_eq!(items[0].label, "/compact");
    assert_eq!(menu::group_items(&items, &typed)[0].label, None);
    assert_eq!(
        items[0].replacement(),
        Some(Replacement::Text("/compact ".into()))
    );
}

#[test]
fn skill_menu_shows_enabled_skills_with_source() {
    let config = config();
    let trigger = prompt::detect_trigger("$", 1).unwrap();
    let items = menu::menu_items(&trigger, Some(codex(&config)), &[]);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "Changelog");
    assert_eq!(items[0].source.as_deref(), Some("Personal"));
    assert_eq!(
        items[0].replacement(),
        Some(Replacement::Chip {
            token: "$changelog".into()
        })
    );
    let none = prompt::detect_trigger("$zzzz", 5).unwrap();
    assert!(menu::menu_items(&none, Some(codex(&config)), &[]).is_empty());
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
    let items = menu::menu_items(&trigger, None, &entries);
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
    let items = menu::menu_items(&trigger, Some(codex(&config)), &[]);
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
    assert!(!send::has_sendable_content("  \u{FFFC} ", 0, 0));
    assert!(send::has_sendable_content("\u{FFFC}", 0, 1));
    assert!(send::has_sendable_content("", 1, 0));
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
