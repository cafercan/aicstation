//! Embedded price table and context-window lookup. Costs are estimates.
use crate::model::Usage;

/// One row: (model id, input, output, cache read, cache write 5m, cache write 1h) in USD per 1M tokens.
///
/// source: `claude-api` skill bundled model table (cached 2026-10-06) for input/output;
/// cache read from the skill's models.md / prompt-caching.md (Fable 5.1 $0.25, Opus 5.5 and
/// Sonnet 5.5 $0.20, Fable 5 $1.00); every other cache read = 0.1x input (skill: "~0.1x"),
/// cache write 5m = 1.25x input and cache write 1h = 2x input (skill prompt-caching.md, "Economics"). Checked 2026-11-02.
/// Haiku 5.5 is priced for prompts up to 100K tokens; longer prompts cost more (not modelled).
const PRICES: &[(&str, f64, f64, f64, f64, f64)] = &[
    ("claude-fable-5-1", 10.0, 50.0, 0.25, 12.5, 20.0),
    ("claude-mythos-5-1", 10.0, 50.0, 0.25, 12.5, 20.0),
    ("claude-fable-5", 10.0, 50.0, 1.0, 12.5, 20.0),
    ("claude-opus-5-5", 4.0, 20.0, 0.20, 5.0, 8.0),
    ("claude-opus-5", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-8", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-7", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-opus-4-6", 5.0, 25.0, 0.5, 6.25, 10.0),
    ("claude-sonnet-5-5", 2.0, 10.0, 0.20, 2.5, 4.0),
    ("claude-sonnet-5", 2.0, 10.0, 0.20, 2.5, 4.0),
    ("claude-sonnet-4-6", 3.0, 15.0, 0.3, 3.75, 6.0),
    ("claude-haiku-5-5", 0.10, 0.50, 0.01, 0.125, 0.2),
    ("claude-haiku-4-5", 1.0, 5.0, 0.1, 1.25, 2.0),
];

fn base_id(model: &str) -> &str {
    model.split('[').next().unwrap_or(model).trim()
}

/// Context windows by model family (source: `claude-api` skill model table, cached 2026-10-06).
/// Matched against the id with `[1m]` and a `-YYYYMMDD` date stripped; the longest family that
/// equals the id or prefixes it at a `-` boundary wins, so `claude-opus-4` never shadows `claude-opus-4-8`.
const CONTEXT_WINDOWS: &[(&str, u64)] = &[
    ("claude-fable-5-1", 1_000_000),
    ("claude-fable-5", 1_000_000),
    ("claude-mythos", 1_000_000),
    ("claude-opus-5-5", 1_000_000),
    ("claude-opus-5", 1_000_000),
    ("claude-opus-4-8", 1_000_000),
    ("claude-opus-4-7", 1_000_000),
    ("claude-opus-4-6", 1_000_000),
    ("claude-sonnet-5-5", 1_000_000),
    ("claude-sonnet-5", 1_000_000),
    ("claude-sonnet-4-6", 1_000_000),
    ("claude-haiku-5-5", 1_000_000),
    ("claude-haiku-4-5", 200_000),
    ("claude-opus-4-5", 200_000),
    ("claude-opus-4-1", 200_000),
    ("claude-opus-4", 200_000),
    ("claude-sonnet-4-5", 200_000),
    ("claude-sonnet-4", 200_000),
    ("claude-3", 200_000),
];

/// Drops a trailing `-YYYYMMDD` date.
fn strip_date(id: &str) -> &str {
    match id.rsplit_once('-') {
        Some((head, d)) if d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => id,
    }
}

/// Context window in tokens: `[1m]` suffix -> 1M, otherwise by model family; unknown -> None.
pub fn context_window(requested_model: &str) -> Option<u64> {
    if requested_model.contains("[1m]") {
        return Some(1_000_000);
    }
    let id = strip_date(base_id(requested_model));
    CONTEXT_WINDOWS
        .iter()
        .filter(|(fam, _)| {
            id == *fam || id.strip_prefix(fam).is_some_and(|rest| rest.starts_with('-'))
        })
        .max_by_key(|(fam, _)| fam.len())
        .map(|&(_, w)| w)
}

/// Estimated cost in USD, or None for an unknown model. A `[1m]` suffix is ignored.
pub fn cost_usd(model: &str, u: &Usage) -> Option<f64> {
    let id = base_id(model);
    let &(_, inp, out, cr, cw5, cw1) = PRICES.iter().find(|r| r.0 == id)?;
    let m = 1_000_000.0;
    // The 1h part is a subset of the total; clamp defensively.
    let c1h = u.cache_creation_1h.min(u.cache_creation);
    Some(
        (u.input as f64 * inp
            + u.output as f64 * out
            + u.cache_read as f64 * cr
            + (u.cache_creation - c1h) as f64 * cw5
            + c1h as f64 * cw1)
            / m,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_known_and_unknown() {
        let u = Usage { input: 1_000_000, output: 1_000_000, cache_read: 0, cache_creation: 0, cache_creation_1h: 0 };
        assert_eq!(cost_usd("claude-opus-5-5[1m]", &u), Some(24.0));
        assert_eq!(cost_usd("gpt-x", &u), None);
        assert_eq!(context_window("foo"), None);
    }

    #[test]
    fn context_window_by_family() {
        for id in [
            "claude-fable-5-1", "claude-fable-5", "claude-mythos-5-1", "claude-mythos-preview",
            "claude-opus-5-5", "claude-opus-5", "claude-opus-4-8", "claude-opus-4-7", "claude-opus-4-6",
            "claude-sonnet-5-5", "claude-sonnet-5", "claude-sonnet-4-6", "claude-haiku-5-5",
        ] {
            assert_eq!(context_window(id), Some(1_000_000), "{id}");
        }
        for id in [
            "claude-haiku-4-5", "claude-opus-4-5", "claude-opus-4-1", "claude-opus-4",
            "claude-sonnet-4-5", "claude-sonnet-4", "claude-3-5-sonnet", "claude-3-opus-20240229",
        ] {
            assert_eq!(context_window(id), Some(200_000), "{id}");
        }
    }

    #[test]
    fn context_window_suffixes_and_unknown() {
        assert_eq!(context_window("claude-haiku-4-5-20251001"), Some(200_000));
        assert_eq!(context_window("claude-sonnet-4-5[1m]"), Some(1_000_000));
        assert_eq!(context_window("claude-opus-5-5[1m]"), Some(1_000_000));
        assert_eq!(context_window("claude-unknown-9"), None);
        assert_eq!(context_window("claude-opus-6"), None);
        assert_eq!(context_window(""), None);
    }

    #[test]
    fn one_hour_cache_writes_use_2x_rate() {
        // opus-5-5: 5m write 5.0, 1h write 8.0 per 1M
        let u = Usage { input: 0, output: 0, cache_read: 0, cache_creation: 2_000_000, cache_creation_1h: 1_000_000 };
        assert_eq!(cost_usd("claude-opus-5-5", &u), Some(13.0));
        // nested split absent -> entire total priced as 5-minute
        let u = Usage { cache_creation: 2_000_000, ..Default::default() };
        assert_eq!(cost_usd("claude-opus-5-5", &u), Some(10.0));
    }
}
