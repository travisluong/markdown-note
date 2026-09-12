use super::*;
use std::{
    borrow::Cow,
    io::Read,
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_RESULTS: usize = 2000;

#[derive(Clone, Debug)]
pub struct Hit {
    pub relative: PathBuf,
    pub range: Range<usize>,
    pub line: usize,
    pub before: String,
    pub matched: String,
    pub after: String,
}

#[derive(Clone)]
pub struct Destination {
    pub path: PathBuf,
    pub hit: Hit,
    pub query: String,
    pub case_sensitive: bool,
}

#[derive(Default)]
pub struct Results {
    pub hits: Vec<Hit>,
    pub skipped: usize,
    pub limited: bool,
}

fn matching_text(text: &str, case_sensitive: bool) -> Cow<'_, str> {
    if case_sensitive {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.to_ascii_lowercase())
    }
}

// ASCII folding preserves byte offsets, matching the editor's Ctrl+F semantics.
pub fn ranges(text: &str, query: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    find_ranges(text, query, case_sensitive, MAX_RESULTS + 1)
}

/// Notebook scans are capped; in-file Find counts and navigates every match.
pub fn find_ranges(
    text: &str,
    query: &str,
    case_sensitive: bool,
    limit: usize,
) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let text = matching_text(text, case_sensitive);
    let query = matching_text(query, case_sensitive);
    text.match_indices(query.as_ref())
        .take(limit)
        .map(|(start, s)| start..start + s.len())
        .collect()
}

fn collect(text: &str, relative: &Path, query: &str, case_sensitive: bool, result: &mut Results) {
    let matches = ranges(text, query, case_sensitive);
    let mut previous = 0;
    let mut line = 1;
    for range in matches {
        if result.hits.len() == MAX_RESULTS {
            result.limited = true;
            break;
        }
        line += text[previous..range.start]
            .bytes()
            .filter(|b| *b == b'\n')
            .count();
        previous = range.start;
        let start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
        let end = text[range.end..]
            .find('\n')
            .map_or(text.len(), |i| range.end + i);
        let before: String = text[start..range.start]
            .chars()
            .rev()
            .take(60)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        result.hits.push(Hit {
            relative: relative.to_path_buf(),
            line,
            before,
            matched: text[range.clone()].chars().take(120).collect(),
            after: text[range.end..end].chars().take(100).collect(),
            range,
        });
    }
}

pub fn scan(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    buffer: Option<(PathBuf, Rope)>,
    cancelled: &AtomicBool,
) -> Results {
    let mut result = Results::default();
    if query.is_empty() || cancelled.load(Ordering::Relaxed) {
        return result;
    }
    if fs::symlink_metadata(root).map_or(true, |m| notebook::is_link(&m)) {
        result.skipped += 1;
        return result;
    }
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        if cancelled.load(Ordering::Relaxed) {
            return result;
        }
        let entries = match notebook::read_directory(root, &relative) {
            Ok(entries) => entries,
            Err(_) => {
                result.skipped += 1;
                continue;
            }
        };
        for entry in entries {
            if cancelled.load(Ordering::Relaxed) {
                return result;
            }
            if entry.link {
                continue;
            }
            if entry.directory {
                pending.push(entry.relative);
                continue;
            }
            if !notebook::markdown(&entry.relative) {
                continue;
            }
            let path = match notebook::resolve(root, &entry.relative) {
                Ok(path) => path,
                Err(_) => {
                    result.skipped += 1;
                    continue;
                }
            };
            let text = if let Some((_, rope)) = buffer.as_ref().filter(|(p, _)| *p == path) {
                if rope.len() as u64 > MAX_FILE_BYTES {
                    result.skipped += 1;
                    continue;
                }
                rope.to_string()
            } else {
                let read = (|| -> io::Result<String> {
                    let file = fs::File::open(path)?;
                    let mut bytes = Vec::new();
                    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
                    if bytes.len() as u64 > MAX_FILE_BYTES {
                        return Err(io::Error::other("File too large"));
                    }
                    let raw = std::str::from_utf8(
                        bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes),
                    )
                    .map_err(io::Error::other)?;
                    Ok(document::normalize_newlines(raw))
                })();
                match read {
                    Ok(text) => text,
                    Err(_) => {
                        result.skipped += 1;
                        continue;
                    }
                }
            };
            collect(&text, &entry.relative, query, case_sensitive, &mut result);
            if result.limited {
                break;
            }
        }
        if result.limited {
            break;
        }
    }
    result.hits.sort_by(|a, b| {
        a.relative
            .cmp(&b.relative)
            .then(a.range.start.cmp(&b.range.start))
    });
    result
}

pub fn revalidate(text: &str, destination: &Destination) -> Option<Range<usize>> {
    if destination.query.is_empty() {
        return None;
    }
    let text = matching_text(text, destination.case_sensitive);
    let query = matching_text(&destination.query, destination.case_sensitive);
    // A changed file can add matches before the original result. Search the
    // complete text without allocating a result list or applying the scan cap.
    // match_indices and min_by_key retain the earlier occurrence on a tie.
    text.match_indices(query.as_ref())
        .map(|(start, matched)| start..start + matched.len())
        .min_by_key(|range| range.start.abs_diff(destination.hit.range.start))
}

#[cfg(test)]
mod tests {
    use super::{
        AtomicBool, Destination, MAX_RESULTS, Path, Results, Rope, collect, fs, ranges, revalidate,
        scan,
    };
    #[test]
    fn in_file_find_is_not_limited_by_the_notebook_result_cap() {
        let text = "hit ".repeat(MAX_RESULTS + 50);
        assert_eq!(
            super::find_ranges(&text, "hit", false, usize::MAX).len(),
            MAX_RESULTS + 50
        );
        assert_eq!(ranges(&text, "hit", false).len(), MAX_RESULTS + 1);
    }

    #[test]
    fn literal_unicode_and_case() {
        assert_eq!(
            ranges("é Foo foo FOO É", "foo", false),
            vec![3..6, 7..10, 11..14]
        );
        assert_eq!(ranges("é É", "é", false), vec![0..2]);
        assert_eq!(ranges("Foo foo", "foo", true), vec![4..7]);
        assert!(ranges("anything", "", false).is_empty());
        assert_eq!(ranges("aaa", "aa", true), vec![0..2]);
    }
    #[test]
    fn traversal_buffer_and_decoding() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir(root.join("nested")).unwrap();
        fs::create_dir(root.join(".gpui-note")).unwrap();
        fs::write(root.join(".gpui-note/hidden.md"), "hit").unwrap();
        fs::write(root.join("other.txt"), "hit").unwrap();
        fs::write(root.join("bad.md"), [255]).unwrap();
        fs::write(root.join("nested/a.MD"), "\u{feff}first\r\nhit\rhit").unwrap();
        fs::write(root.join("open.md"), "disk hit").unwrap();
        let result = scan(
            &root,
            "hit",
            false,
            Some((root.join("open.md"), Rope::from("unsaved"))),
            &AtomicBool::new(false),
        );
        assert_eq!(result.hits.len(), 2);
        assert_eq!(result.skipped, 1);
        assert_eq!(result.hits[0].range, 6..9);
        assert_eq!(result.hits[1].line, 3);
        assert!(
            scan(&root, "hit", false, None, &AtomicBool::new(true))
                .hits
                .is_empty()
        );
    }
    #[test]
    fn limits_and_changed_result() {
        let mut result = Results::default();
        collect(
            &"x ".repeat(MAX_RESULTS + 2),
            Path::new("a.md"),
            "x",
            true,
            &mut result,
        );
        assert!(result.limited);
        assert_eq!(result.hits.len(), MAX_RESULTS);
        let dest = Destination {
            path: "a.md".into(),
            hit: result.hits[0].clone(),
            query: "x".into(),
            case_sensitive: true,
        };
        assert_eq!(revalidate("moved x", &dest), Some(6..7));
        assert_eq!(revalidate("gone", &dest), None);
    }

    fn destination(query: &str, case_sensitive: bool, start: usize) -> Destination {
        Destination {
            path: "a.md".into(),
            hit: super::Hit {
                relative: "a.md".into(),
                range: start..start + query.len(),
                line: 1,
                before: String::new(),
                matched: query.into(),
                after: String::new(),
            },
            query: query.into(),
            case_sensitive,
        }
    }

    #[test]
    fn revalidation_finds_original_match_beyond_the_notebook_cap() {
        let original = format!("{}hit", " ".repeat(10_000));
        let mut result = Results::default();
        collect(&original, Path::new("a.md"), "hit", true, &mut result);
        let destination = Destination {
            path: "a.md".into(),
            hit: result.hits.remove(0),
            query: "hit".into(),
            case_sensitive: true,
        };
        let prefix = "hit ".repeat(MAX_RESULTS + 2);
        let changed = format!("{prefix}{}hit", " ".repeat(10_000 - prefix.len()));

        assert_eq!(ranges(&changed, "hit", true).len(), MAX_RESULTS + 1);
        assert_eq!(revalidate(&changed, &destination), Some(10_000..10_003));
    }

    #[test]
    fn revalidation_selects_nearest_start_and_the_earlier_tie() {
        let text = "hit----hit----hit";
        for (start, expected) in [(0, 0..3), (5, 7..10), (12, 14..17), (100, 14..17)] {
            assert_eq!(
                revalidate(text, &destination("hit", true, start)),
                Some(expected)
            );
        }
        assert_eq!(
            revalidate("hit---hit", &destination("hit", true, 3)),
            Some(0..3)
        );
        // Literal search retains non-overlapping matches, even when a later
        // overlapping occurrence would be nearer to the previous position.
        assert_eq!(revalidate("aaa", &destination("aa", true, 1)), Some(0..2));
    }

    #[test]
    fn revalidation_preserves_ascii_case_and_unicode_byte_offsets() {
        let text = "é Foo foo FOO É";
        assert_eq!(revalidate(text, &destination("foo", false, 3)), Some(3..6));
        assert_eq!(revalidate(text, &destination("foo", true, 3)), Some(7..10));
        assert_eq!(
            revalidate(text, &destination("FOO", true, 12)),
            Some(11..14)
        );
        assert_eq!(revalidate("é É", &destination("é", false, 3)), Some(0..2));
        assert_eq!(revalidate("é É", &destination("É", false, 0)), Some(3..5));
        assert_eq!(
            revalidate("你好 你好", &destination("你好", false, 8)),
            Some(7..13)
        );
    }

    #[test]
    fn revalidation_rejects_empty_and_absent_queries() {
        assert_eq!(revalidate("anything", &destination("", false, 0)), None);
        assert_eq!(revalidate("", &destination("", true, 0)), None);
        assert_eq!(revalidate("", &destination("hit", false, 0)), None);
        assert_eq!(revalidate("gone", &destination("hit", true, 0)), None);
    }
}
