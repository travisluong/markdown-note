use chrono::{DateTime, Datelike, Local, Offset, TimeZone, Timelike};

pub fn default_format() -> String {
    "YYYY-MM-DD".into()
}

// Longest tokens first. Literal text never passes through strftime.
const TOKENS: &[(&str, &str)] = &[
    ("YYYY", "%Y"),
    ("MMMM", "%B"),
    ("dddd", "%A"),
    ("MMM", "%b"),
    ("ddd", "%a"),
    ("SSS", ""),
    ("YY", "%y"),
    ("MM", "%m"),
    ("DD", "%d"),
    ("Do", ""),
    ("dd", ""),
    ("HH", "%H"),
    ("hh", "%I"),
    ("mm", "%M"),
    ("ss", "%S"),
    ("ZZ", "%z"),
    ("M", "%-m"),
    ("D", "%-d"),
    ("d", "%w"),
    ("H", "%-H"),
    ("h", "%-I"),
    ("m", "%-M"),
    ("s", "%-S"),
    ("A", "%p"),
    ("a", "%P"),
    ("Z", "%:z"),
];

pub fn format<T: TimeZone>(pattern: &str, now: &DateTime<T>) -> Result<String, String>
where
    T::Offset: std::fmt::Display,
{
    if pattern.trim().is_empty() {
        return Err("Enter a date format.".into());
    }
    let mut remaining = pattern;
    let mut result = String::new();
    while let Some(c) = remaining.chars().next() {
        if c == '\\' {
            remaining = &remaining[1..];
            let next = remaining
                .chars()
                .next()
                .ok_or("A backslash must escape a character.")?;
            result.push(next);
            remaining = &remaining[next.len_utf8()..];
        } else if c == '[' {
            remaining = &remaining[1..];
            let mut closed = false;
            while let Some(ch) = remaining.chars().next() {
                remaining = &remaining[ch.len_utf8()..];
                if ch == ']' {
                    closed = true;
                    break;
                }
                if ch == '[' {
                    return Err("Nested literal brackets are not supported.".into());
                }
                if ch == '\\' {
                    let escaped = remaining
                        .chars()
                        .next()
                        .ok_or("A backslash must escape a character.")?;
                    result.push(escaped);
                    remaining = &remaining[escaped.len_utf8()..];
                } else {
                    result.push(ch);
                }
            }
            if !closed {
                return Err("Close the literal with ].".into());
            }
        } else if c == ']' {
            return Err("Unexpected ]; use brackets or a backslash for literal text.".into());
        } else if let Some(&(token, spec)) = TOKENS
            .iter()
            .find(|(token, _)| remaining.starts_with(token))
        {
            match token {
                "Do" => {
                    let day = now.day();
                    let suffix = if (11..=13).contains(&(day % 100)) {
                        "th"
                    } else {
                        match day % 10 {
                            1 => "st",
                            2 => "nd",
                            3 => "rd",
                            _ => "th",
                        }
                    };
                    result.push_str(&format!("{day}{suffix}"));
                }
                "dd" => result.push_str(&now.format("%a").to_string()[..2]),
                "SSS" => result.push_str(&format!("{:03}", now.nanosecond() / 1_000_000)),
                _ => result.push_str(&now.format(spec).to_string()),
            }
            remaining = &remaining[token.len()..];
        } else if c.is_alphabetic() {
            return Err(format!(
                "Unsupported token starting at {remaining:?}. Put literal text in [brackets]."
            ));
        } else {
            result.push(c);
            remaining = &remaining[c.len_utf8()..];
        }
    }
    Ok(result)
}

pub fn current(pattern: &str) -> Result<String, String> {
    let now = Local::now();
    format(pattern, &now.with_timezone(&now.offset().fix()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn timestamp() -> DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339("2026-09-03T14:05:06.789-05:30").unwrap()
    }
    #[test]
    fn all_tokens() {
        assert_eq!(
            format(
                "YY YYYY M MM MMM MMMM D DD Do d dd ddd dddd H HH h hh m mm s ss SSS A a Z ZZ",
                &timestamp()
            )
            .unwrap(),
            "26 2026 9 09 Sep September 3 03 3rd 4 Th Thu Thursday 14 14 2 02 5 05 6 06 789 PM pm -05:30 -0530"
        );
    }
    #[test]
    fn literals_and_validation() {
        assert_eq!(
            format(r"[Today] YYYY\Y [日本語] 100%", &timestamp()).unwrap(),
            "Today 2026Y 日本語 100%"
        );
        for bad in ["", "  ", "Q", "[oops", "oops]", "YYYY\\", "[a[b]]", "SS"] {
            assert!(format(bad, &timestamp()).is_err(), "{bad}");
        }
        for (day, suffix) in [
            (1, "st"),
            (2, "nd"),
            (3, "rd"),
            (11, "th"),
            (12, "th"),
            (13, "th"),
            (21, "st"),
            (22, "nd"),
            (23, "rd"),
            (31, "st"),
        ] {
            let date = timestamp().with_month(1).unwrap().with_day(day).unwrap();
            assert_eq!(format("Do", &date).unwrap(), format!("{day}{suffix}"));
        }
        let midnight = timestamp().with_hour(0).unwrap();
        assert_eq!(format("H HH h hh A", &midnight).unwrap(), "0 00 12 12 AM");
    }
}
