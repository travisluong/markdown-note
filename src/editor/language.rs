//! File-name language selection. Unknown UTF-8 files remain editable plain text.
use gpui_component::highlighter::Language;
use std::path::Path;

pub(super) fn for_path(path: &Path) -> &'static str {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let language = match name.as_str() {
        "makefile" | "gnumakefile" => "make",
        "cmakelists.txt" => "cmake",
        ".bashrc" | ".bash_profile" | ".profile" => "bash",
        _ => match extension.as_str() {
            "mjs" | "cjs" | "jsx" => "javascript",
            "mts" | "cts" => "typescript",
            "h" => "c",
            "cc" | "cxx" | "hpp" | "hxx" => "cpp",
            "htm" => "html",
            "bash" | "zsh" => "bash",
            "gql" => "graphql",
            "exs" => "elixir",
            "patch" => "diff",
            // Only .md and .markdown have Markdown semantics in Mithril.
            "mdx" => "text",
            other => other,
        },
    };
    Language::from_str(language).name()
}

#[test]
fn common_file_names_select_supported_grammars() {
    for (name, language) in [
        ("note.MD", "markdown"),
        ("file.txt", "text"),
        ("file.js", "javascript"),
        ("file.mjs", "javascript"),
        ("file.ts", "typescript"),
        ("file.tsx", "tsx"),
        ("script.sh", "bash"),
        ("Makefile", "make"),
        ("CMakeLists.txt", "cmake"),
        (".bashrc", "bash"),
        ("unknown.xyz", "text"),
        ("README", "text"),
    ] {
        assert_eq!(for_path(Path::new(name)), language, "{name}");
    }
}
