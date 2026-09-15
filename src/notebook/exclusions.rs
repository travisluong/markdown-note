//! Notebook-relative discovery policy, independent of filesystem access guards.
use regex::Regex;
use std::{path::Path, sync::Arc};

#[derive(Clone, Debug, Default)]
pub struct Exclusions(Arc<[Regex]>);

impl Exclusions {
    pub fn compile(patterns: &[String]) -> Result<Self, String> {
        patterns
            .iter()
            .enumerate()
            .map(|(index, pattern)| {
                Regex::new(pattern).map_err(|error| {
                    format!(
                        "Invalid exclusion pattern {} ({pattern}): {error}",
                        index + 1
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|patterns| Self(patterns.into()))
    }

    /// Test the entry and every folder ancestor, never the notebook root.
    pub fn excludes(&self, relative: &Path) -> bool {
        if self.0.is_empty() {
            return false;
        }
        relative.ancestors().any(|path| {
            if path.as_os_str().is_empty() {
                return false;
            }
            let normalized = path
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            self.0.iter().any(|pattern| pattern.is_match(&normalized))
        })
    }
}

/// Commas delimit patterns except when escaped, inside a class, or inside braces.
/// Regex compilation validates incomplete classes, repetitions and escapes.
pub fn parse_excluded_patterns(value: &str) -> Result<Vec<String>, String> {
    let mut patterns = Vec::new();
    let mut start = 0;
    let mut escaped = false;
    // Class prefix: 0 = initial, 1 = after negation, 2 = at least one member.
    let mut classes = Vec::new();
    let mut braces = 0usize;
    for (index, ch) in value.char_indices() {
        if escaped {
            escaped = false;
            if let Some(initial) = classes.last_mut() {
                *initial = 2;
            }
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(initial) = classes.last_mut() {
            match ch {
                ']' if *initial == 2 => {
                    classes.pop();
                }
                '[' => {
                    *initial = 2;
                    classes.push(0u8);
                }
                '^' if *initial == 0 => *initial = 1,
                _ => *initial = 2,
            }
            continue;
        }
        match ch {
            '[' => classes.push(0u8),
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            ',' if braces == 0 => {
                push_pattern(&mut patterns, &value[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    push_pattern(&mut patterns, &value[start..]);
    Exclusions::compile(&patterns)?;
    Ok(patterns)
}

fn push_pattern(patterns: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        patterns.push(value.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_regex_commas_and_validates_the_whole_list() {
        let input = r" , ^archive$, a{1,3}, [a,b], report\,old, [a-z&&[^x,y]], , ";
        let patterns = parse_excluded_patterns(input).unwrap();
        assert_eq!(
            patterns,
            [
                r"^archive$",
                r"a{1,3}",
                r"[a,b]",
                r"report\,old",
                r"[a-z&&[^x,y]]"
            ]
        );
        assert_eq!(
            parse_excluded_patterns(&patterns.join(", ")).unwrap(),
            patterns
        );
        assert!(parse_excluded_patterns(" , , ").unwrap().is_empty());
        assert_eq!(parse_excluded_patterns(r"a\\, b").unwrap(), [r"a\\", "b"]);
        assert_eq!(
            parse_excluded_patterns("[],], next").unwrap(),
            ["[],]", "next"]
        );
        assert_eq!(
            parse_excluded_patterns("[^^], next").unwrap(),
            ["[^^]", "next"]
        );
        for invalid in ["good, [broken", "good, a{1,", "good, (", "good, \\"] {
            assert!(
                parse_excluded_patterns(invalid)
                    .unwrap_err()
                    .contains("pattern 2")
            );
        }
    }

    #[test]
    fn matches_relative_paths_and_folder_ancestors() {
        let patterns =
            parse_excluded_patterns(r"^archive$, \.tmp$, (^|/)drafts$, (?i)^private/").unwrap();
        let policy = Exclusions::compile(&patterns).unwrap();
        for path in [
            "archive",
            "archive/deep/note.md",
            "nested/file.tmp",
            "nested/drafts/note.md",
            "Private/Note.md",
        ] {
            assert!(policy.excludes(Path::new(path)), "{path}");
        }
        for path in [
            "",
            "archives/note.md",
            "Archive/note.md",
            "nested/archive/note.md",
            "note.md",
            "note.TMP",
        ] {
            assert!(!policy.excludes(Path::new(path)), "{path}");
        }
        assert!(policy.excludes(&Path::new("nested").join("drafts").join("note.md")));
        assert!(!Exclusions::default().excludes(Path::new("archive/note.md")));
    }
}
