//! Relative time phrases in recall queries.
//!
//! "What did I buy two weeks ago?" names a time that embeddings and BM25 ignore.
//! `parse_time_anchor` turns the phrase into a [`TimeAnchor`] relative to the
//! question's reference time, and `anchor_boost` scores how close a memory is to it.

use crate::types::TimeAnchor;

const DAY: i64 = 86_400;

/// Largest score multiplier a memory gets for being exactly at the anchor.
pub const TEMPORAL_BOOST: f32 = 0.4;

/// Score multiplier for a memory created at `created_at`: `1.0` outside the
/// anchor window, rising linearly to `1.0 + TEMPORAL_BOOST` at the target.
pub fn anchor_boost(anchor: &TimeAnchor, created_at: i64) -> f32 {
    if anchor.window_secs <= 0 {
        return 1.0;
    }
    let distance = (created_at - anchor.target).unsigned_abs() as f64;
    let proximity = (1.0 - distance / anchor.window_secs as f64).max(0.0);
    1.0 + TEMPORAL_BOOST * proximity as f32
}

/// Find the first relative time phrase in `query` and resolve it against
/// `reference_time` (unix seconds). Returns `None` when the query has none.
///
/// Recognised: "yesterday", "today", "recently", "lately", "last week|month|year",
/// and "<n> day(s)|week(s)|month(s)|year(s) ago" where `<n>` is a digit, a number
/// word up to twelve, "a", "an", "a couple of" or "a few".
pub fn parse_time_anchor(query: &str, reference_time: i64) -> Option<TimeAnchor> {
    let lower = query.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();

    for (i, token) in tokens.iter().enumerate() {
        let span = match *token {
            "yesterday" => Some((DAY, DAY)),
            "today" => Some((0, DAY)),
            "recently" | "lately" => Some((0, 14 * DAY)),
            "last" => tokens.get(i + 1).and_then(|unit| unit_span(unit, 1)),
            "ago" if i >= 2 => {
                count_before(&tokens[..i - 1]).and_then(|count| unit_span(tokens[i - 1], count))
            }
            _ => None,
        };
        if let Some((offset, window_secs)) = span {
            return Some(TimeAnchor {
                target: reference_time - offset,
                window_secs,
            });
        }
    }
    None
}

/// `(offset, window)` in seconds for `count` of a unit word.
fn unit_span(unit: &str, count: i64) -> Option<(i64, i64)> {
    match unit.trim_end_matches('s') {
        "day" => Some((count * DAY, 2 * DAY)),
        "week" => Some((count * 7 * DAY, 7 * DAY)),
        "month" => Some((count * 30 * DAY, 15 * DAY)),
        "year" => Some((count * 365 * DAY, 120 * DAY)),
        _ => None,
    }
}

/// The count that ends `tokens`: "3", "three", "a", "a couple of", "a few".
fn count_before(tokens: &[&str]) -> Option<i64> {
    let last = *tokens.last()?;
    if last == "of" {
        let before_of = tokens.len().checked_sub(2).map(|i| tokens[i]);
        return (before_of == Some("couple")).then_some(2);
    }
    if let Ok(n) = last.parse::<i64>() {
        return (n > 0 && n < 10_000).then_some(n);
    }
    let n = match last {
        "a" | "an" | "one" => 1,
        "two" => 2,
        "three" | "few" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        _ => return None,
    };
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn anchor(query: &str) -> Option<TimeAnchor> {
        parse_time_anchor(query, NOW)
    }

    #[test]
    fn numeric_and_word_counts() {
        assert_eq!(
            anchor("what did I do 3 days ago?").unwrap().target,
            NOW - 3 * DAY
        );
        assert_eq!(
            anchor("Four weeks ago I started").unwrap().target,
            NOW - 28 * DAY
        );
        assert_eq!(anchor("a month ago").unwrap().target, NOW - 30 * DAY);
        assert_eq!(anchor("a year ago").unwrap().target, NOW - 365 * DAY);
        assert_eq!(
            anchor("a couple of days ago").unwrap().target,
            NOW - 2 * DAY
        );
        assert_eq!(anchor("a few weeks ago").unwrap().target, NOW - 21 * DAY);
    }

    #[test]
    fn named_phrases() {
        assert_eq!(anchor("yesterday's meeting").unwrap().target, NOW - DAY);
        assert_eq!(
            anchor("what happened last week").unwrap().target,
            NOW - 7 * DAY
        );
        assert_eq!(anchor("last month").unwrap().window_secs, 15 * DAY);
        let recent = anchor("what have we discussed recently").unwrap();
        assert_eq!(recent.target, NOW);
        assert_eq!(recent.window_secs, 14 * DAY);
    }

    #[test]
    fn no_phrase() {
        assert_eq!(anchor("what is alice's favourite colour"), None);
        assert_eq!(anchor("the last time we spoke"), None);
        assert_eq!(anchor("ago"), None);
        assert_eq!(anchor("days ago"), None);
        assert_eq!(anchor("many days ago"), None);
    }

    #[test]
    fn boost_falls_off_linearly() {
        let a = TimeAnchor {
            target: NOW,
            window_secs: 10 * DAY,
        };
        assert!((anchor_boost(&a, NOW) - (1.0 + TEMPORAL_BOOST)).abs() < 1e-6);
        assert!((anchor_boost(&a, NOW - 5 * DAY) - (1.0 + TEMPORAL_BOOST / 2.0)).abs() < 1e-6);
        assert!((anchor_boost(&a, NOW + 5 * DAY) - (1.0 + TEMPORAL_BOOST / 2.0)).abs() < 1e-6);
        assert_eq!(anchor_boost(&a, NOW - 10 * DAY), 1.0);
        assert_eq!(anchor_boost(&a, NOW - 400 * DAY), 1.0);
    }

    #[test]
    fn zero_window_gives_no_boost() {
        let a = TimeAnchor {
            target: NOW,
            window_secs: 0,
        };
        assert_eq!(anchor_boost(&a, NOW), 1.0);
    }
}
