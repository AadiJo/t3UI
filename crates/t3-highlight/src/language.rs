//! Language resolution matching what the fork feeds Shiki.
//!
//! Fence info strings resolve through Shiki's ids and aliases; file paths through
//! `@pierre/diffs`' extension table (both exported to `assets/languages.tsv`). The Shiki language
//! is then mapped to the closest syntect grammar. Anything unresolved is plain text, which is
//! what Shiki does for unsupported languages.

use std::{collections::HashMap, sync::LazyLock};

use crate::engine;

/// A resolved grammar, or plain text. Cheap to copy and compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Language(Option<Grammar>);

/// Where a grammar comes from: Shiki's own grammar translated for syntect (`assets/grammars`), or
/// bat's Sublime grammar set (two-face). Indexes into the respective syntax set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Grammar {
    Shiki(u16),
    Bat(u16),
}

/// A grammar by name in one of the two sets.
#[derive(Clone, Copy, Debug)]
enum Source {
    Shiki(&'static str),
    Bat(&'static str),
}

impl Language {
    /// Plain text: everything in the theme foreground.
    pub const PLAIN: Self = Self(None);

    /// Resolves a code fence info string (` ```ts title="a.ts" `). Only the first word counts,
    /// case-insensitively, with the fork's `gitignore` -> `ini` substitution.
    pub fn from_fence(info: &str) -> Self {
        let Some(word) = info.split_whitespace().next() else {
            return Self::PLAIN;
        };
        let word = word.to_ascii_lowercase();
        let word = if word == "gitignore" { "ini" } else { &word };
        TABLES
            .aliases
            .get(word)
            .map_or(Self::PLAIN, |shiki| Self::from_shiki_id(shiki))
    }

    /// Resolves a file path the way `@pierre/diffs`' `getFiletypeFromFileName` does: exact file
    /// name, then a compound extension (`component.ts`), then the last extension.
    pub fn from_path(path: &str) -> Self {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let extensions = &TABLES.extensions;
        let shiki = extensions
            .get(name)
            .or_else(|| {
                // `/\.([^/\\]+\.[^/\\]+)$/`: everything after the first dot, if it has another.
                let compound = &name[name.find('.')? + 1..];
                compound.contains('.').then_some(())?;
                extensions.get(compound)
            })
            .or_else(|| {
                let extension = &name[name.rfind('.')? + 1..];
                extensions.get(extension)
            });
        // The table holds Shiki aliases too (`Dockerfile` -> `dockerfile`, `sh` -> `zsh`).
        shiki.map_or(Self::PLAIN, |shiki| {
            Self::from_shiki_id(TABLES.aliases.get(shiki.as_str()).unwrap_or(shiki))
        })
    }

    /// Whether this renders as plain text.
    pub fn is_plain(self) -> bool {
        self.0.is_none()
    }

    /// The grammar's display name (e.g. "TypeScript"), or "Plain Text".
    pub fn name(self) -> &'static str {
        match self.0 {
            Some(grammar) => engine::syntax(grammar).1.name.as_str(),
            None => "Plain Text",
        }
    }

    pub(crate) fn grammar(self) -> Option<Grammar> {
        self.0
    }

    fn from_shiki_id(shiki: &str) -> Self {
        Self(
            SHIKI_TO_SYNTECT
                .iter()
                .find(|(id, _)| *id == shiki)
                .and_then(|(_, source)| source.resolve()),
        )
    }
}

impl Source {
    fn resolve(self) -> Option<Grammar> {
        match self {
            Self::Shiki(name) => engine::shiki_syntax_index(name).map(Grammar::Shiki),
            Self::Bat(name) => engine::bat_syntax_index(name).map(Grammar::Bat),
        }
    }
}

struct Tables {
    /// Shiki language id or alias (lowercase) -> Shiki language id.
    aliases: HashMap<String, String>,
    /// File name or extension (case-sensitive, as in `@pierre/diffs`) -> Shiki language id.
    extensions: HashMap<String, String>,
}

static TABLES: LazyLock<Tables> = LazyLock::new(|| {
    let mut tables = Tables {
        aliases: HashMap::new(),
        extensions: HashMap::new(),
    };
    for row in include_str!("../assets/languages.tsv").lines() {
        let mut fields = row.split('\t');
        let (Some(kind), Some(key), Some(shiki)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let map = match kind {
            "alias" => &mut tables.aliases,
            "ext" => &mut tables.extensions,
            _ => continue,
        };
        map.insert(key.to_string(), shiki.to_string());
    }
    tables
});

/// Shiki language id -> grammar. `Source::Shiki` names a file in `assets/grammars`;
/// `Source::Bat` names a grammar in two-face's bat set (fancy-regex build).
///
/// JavaScript and JSX use the TypeScript grammars on purpose: VS Code (and so Shiki) generates its
/// JS/JSX grammars from the TypeScript ones, so their scopes match Pierre's rules far better than
/// Sublime's own JavaScript grammar does.
const SHIKI_TO_SYNTECT: &[(&str, Source)] = &[
    ("actionscript-3", Source::Bat("ActionScript")),
    ("ada", Source::Bat("Ada")),
    ("apache", Source::Bat("Apache Conf")),
    ("applescript", Source::Bat("AppleScript")),
    ("asciidoc", Source::Bat("AsciiDoc (Asciidoctor)")),
    ("asm", Source::Bat("x86_64 Assembly")),
    ("awk", Source::Bat("AWK")),
    ("bat", Source::Bat("Batch File")),
    ("bibtex", Source::Bat("BibTeX")),
    ("c", Source::Bat("C")),
    ("clojure", Source::Bat("Clojure")),
    ("cmake", Source::Bat("CMake")),
    ("coffee", Source::Bat("CoffeeScript")),
    ("common-lisp", Source::Bat("Lisp")),
    ("cpp", Source::Bat("C++")),
    ("crystal", Source::Bat("Crystal")),
    ("csharp", Source::Bat("C#")),
    ("css", Source::Bat("CSS")),
    ("csv", Source::Bat("Separated Values")),
    ("d", Source::Bat("D")),
    ("dart", Source::Bat("Dart")),
    ("diff", Source::Shiki("diff")),
    ("docker", Source::Bat("Dockerfile")),
    ("dotenv", Source::Bat("DotENV")),
    ("elixir", Source::Bat("Elixir")),
    ("elm", Source::Bat("Elm")),
    ("erb", Source::Bat("HTML (Rails)")),
    ("erlang", Source::Bat("Erlang")),
    ("fish", Source::Bat("Fish")),
    ("fortran-fixed-form", Source::Bat("Fortran (Fixed Form)")),
    ("fortran-free-form", Source::Bat("Fortran (Modern)")),
    ("fsharp", Source::Bat("F#")),
    ("gdscript", Source::Bat("GDScript (Godot Engine)")),
    ("git-commit", Source::Bat("Git Commit")),
    ("git-rebase", Source::Bat("Git Rebase Todo")),
    ("glsl", Source::Bat("GLSL")),
    ("go", Source::Bat("Go")),
    ("graphql", Source::Bat("GraphQL")),
    ("groovy", Source::Bat("Groovy")),
    ("haml", Source::Bat("Ruby Haml")),
    ("haskell", Source::Bat("Haskell")),
    ("hcl", Source::Bat("Terraform")),
    ("html", Source::Bat("HTML")),
    ("http", Source::Bat("HTTP Request and Response")),
    ("ini", Source::Bat("INI")),
    ("java", Source::Bat("Java")),
    ("javascript", Source::Bat("TypeScript")),
    ("jinja", Source::Bat("Jinja2")),
    ("json", Source::Shiki("json")),
    ("json5", Source::Shiki("json")),
    ("jsonc", Source::Shiki("json")),
    ("jsonl", Source::Shiki("json")),
    ("jsonnet", Source::Bat("jsonnet")),
    ("jsx", Source::Bat("TypeScriptReact")),
    ("julia", Source::Bat("Julia")),
    ("kotlin", Source::Bat("Kotlin")),
    ("latex", Source::Bat("LaTeX")),
    ("lean", Source::Bat("Lean 4")),
    ("less", Source::Bat("Less")),
    ("llvm", Source::Bat("LLVM")),
    ("log", Source::Bat("log")),
    ("lua", Source::Bat("Lua")),
    ("make", Source::Bat("Makefile")),
    ("markdown", Source::Bat("Markdown")),
    ("matlab", Source::Bat("MATLAB")),
    ("nginx", Source::Bat("nginx")),
    ("nim", Source::Bat("Nim")),
    ("nix", Source::Bat("Nix")),
    ("objective-c", Source::Bat("Objective-C")),
    ("objective-cpp", Source::Bat("Objective-C++")),
    ("ocaml", Source::Bat("OCaml")),
    ("odin", Source::Bat("Odin")),
    ("pascal", Source::Bat("Pascal")),
    ("perl", Source::Bat("Perl")),
    ("php", Source::Bat("PHP")),
    ("proto", Source::Bat("Protocol Buffer")),
    ("puppet", Source::Bat("Puppet")),
    ("purescript", Source::Bat("PureScript")),
    ("python", Source::Shiki("python")),
    ("qml", Source::Bat("QML")),
    ("r", Source::Bat("R")),
    ("racket", Source::Bat("Racket")),
    ("regexp", Source::Bat("Regular Expression")),
    ("rst", Source::Bat("reStructuredText")),
    ("ruby", Source::Bat("Ruby")),
    ("rust", Source::Shiki("rust")),
    ("sass", Source::Bat("Sass")),
    ("scala", Source::Bat("Scala")),
    ("scss", Source::Bat("SCSS")),
    ("shellscript", Source::Bat("Bourne Again Shell (bash)")),
    ("solidity", Source::Bat("Solidity")),
    ("sql", Source::Bat("SQL")),
    ("ssh-config", Source::Bat("SSH Config")),
    ("stylus", Source::Bat("Stylus")),
    ("svelte", Source::Bat("Svelte")),
    ("swift", Source::Bat("Swift")),
    ("system-verilog", Source::Bat("SystemVerilog")),
    ("tcl", Source::Bat("Tcl")),
    ("terraform", Source::Bat("Terraform")),
    ("tex", Source::Bat("TeX")),
    ("toml", Source::Bat("TOML")),
    ("tsv", Source::Bat("Tab Separated Values")),
    ("tsx", Source::Bat("TypeScriptReact")),
    ("twig", Source::Bat("HTML (Twig)")),
    ("typescript", Source::Bat("TypeScript")),
    ("typst", Source::Bat("Typst")),
    ("verilog", Source::Bat("Verilog")),
    ("vhdl", Source::Bat("VHDL")),
    ("viml", Source::Bat("VimL")),
    ("vue", Source::Bat("Vue Component")),
    ("vyper", Source::Bat("Vyper")),
    ("wgsl", Source::Bat("WGSL")),
    ("wikitext", Source::Bat("MediaWiki")),
    ("xml", Source::Bat("XML")),
    ("yaml", Source::Bat("YAML")),
    ("zig", Source::Bat("Zig")),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A typo in the table would silently turn a language into plain text.
    #[test]
    fn every_mapped_grammar_exists() {
        for (shiki, source) in SHIKI_TO_SYNTECT {
            assert!(source.resolve().is_some(), "{shiki} -> {source:?}");
            assert!(
                TABLES.aliases.values().any(|id| id == shiki),
                "{shiki} is not a Shiki language id"
            );
        }
    }
}
