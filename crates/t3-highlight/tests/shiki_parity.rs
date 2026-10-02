//! Measures how closely t3-highlight reproduces the fork's Shiki output.
//!
//! `tools/shiki-fixtures.mjs` records Shiki's colors for `tests/fixtures/samples/*`. This test
//! compares every non-whitespace character and fails when a language drops below its floor.
//! Run with `--nocapture` to see the per-language score and the mismatched tokens.
//! The floors encode known grammar gaps (syntect runs Sublime grammars, Shiki runs VS Code's),
//! so raising one is good and lowering one needs a reason.

use std::{fs, path::Path};

use t3_highlight::{Language, Theme, highlight};

/// Minimum share of non-whitespace characters whose color matches Shiki.
fn floor(lang: &str) -> f64 {
    match lang {
        // bat's Bash grammar plus scope aliases; Shiki's grammar needs `\G`, which syntect lacks.
        "bash" => 0.85,
        _ => 0.97,
    }
}

fn parse_color(hex: &str) -> u32 {
    let hex = hex.trim_start_matches('#');
    let value = u32::from_str_radix(hex, 16).expect("hex color");
    if hex.len() == 6 {
        value << 8 | 0xff
    } else {
        value
    }
}

#[test]
fn shiki_parity() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut entries: Vec<_> = fs::read_dir(fixtures.join("shiki"))
        .expect("fixtures exist")
        .map(|entry| entry.expect("entry").path())
        .collect();
    entries.sort();
    let mut failures = Vec::new();

    for path in entries {
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        let (sample, theme_name) = file_name
            .trim_end_matches(".json")
            .rsplit_once('.')
            .expect("<sample>.<theme>.json");
        let theme = if theme_name == "pierre-dark" {
            Theme::Dark
        } else {
            Theme::Light
        };
        let fixture: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let lang = fixture["lang"].as_str().unwrap();
        let code = fs::read_to_string(fixtures.join("samples").join(sample)).unwrap();

        // Shiki's expected color for each byte offset, from its per-line tokens.
        let mut expected = vec![None; code.len()];
        let line_starts = std::iter::once(0).chain(code.match_indices('\n').map(|(ix, _)| ix + 1));
        for (line, start) in fixture["lines"].as_array().unwrap().iter().zip(line_starts) {
            let mut offset = start;
            for token in line.as_array().unwrap() {
                let content = token[0].as_str().unwrap();
                let color = parse_color(token[1].as_str().unwrap());
                for slot in &mut expected[offset..offset + content.len()] {
                    *slot = Some(color);
                }
                offset += content.len();
            }
        }

        let ours = highlight(&code, Language::from_fence(lang), theme);
        let mut actual = vec![0u32; code.len()];
        for (range, style) in ours.spans() {
            actual[range].fill(style.color);
        }

        let mut total = 0usize;
        let mut matching = 0usize;
        let mut mismatches: Vec<(String, u32, u32)> = Vec::new();
        for (offset, ch) in code.char_indices() {
            let Some(want) = expected[offset] else {
                continue;
            };
            if ch.is_whitespace() {
                continue;
            }
            total += 1;
            let got = actual[offset];
            if got == want {
                matching += 1;
                continue;
            }
            match mismatches.last_mut() {
                Some((text, w, g)) if *w == want && *g == got => text.push(ch),
                _ => mismatches.push((ch.to_string(), want, got)),
            }
        }
        let score = matching as f64 / total.max(1) as f64;
        println!(
            "{sample} {theme_name}: {:.1}% of {total} chars",
            score * 100.0
        );
        for (text, want, got) in mismatches.iter().take(12) {
            println!("    {text:?}: shiki #{want:08x}, ours #{got:08x}");
        }
        if score < floor(lang) {
            failures.push(format!(
                "{sample} {theme_name}: {:.3} < {}",
                score,
                floor(lang)
            ));
        }
    }
    assert!(failures.is_empty(), "below parity floor: {failures:#?}");
}
