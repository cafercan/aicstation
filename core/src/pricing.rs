//! Embedded price table and context-window lookup. Costs are estimates.
use crate::model::Usage;

/// One row: (model id, input, output, cache read, cache write) in USD per 1M tokens.
///
/// source: `claude-api` skill bundled model table (cached 2026-10-06) for input/output;
/// cache read from the skill's models.md / prompt-caching.md (Fable 5.1 $0.25, Opus 5.5 and
/// Sonnet 5.5 $0.20, Fable 5 $1.00); every other cache read = 0.1x input (skill: "~0.1x"),
/// cache write = 1.25x input (5-minute TTL; skill prompt-caching.md). Checked 2026-11-02.
/// Haiku 5.5 is priced for prompts up to 100K tokens; longer prompts cost more (not modelled).
const PRICES: &[(&str, f64, f64, f64, f64)] = &[
    ("claude-fable-5-1", 10.0, 50.0, 0.25, 12.5),
    ("claude-mythos-5-1", 10.0, 50.0, 0.25, 12.5),
    ("claude-fable-5", 10.0, 50.0, 1.0, 12.5),
    ("claude-opus-5-5", 4.0, 20.0, 0.20, 5.0),
    ("claude-opus-5", 5.0, 25.0, 0.5, 6.25),
    ("claude-opus-4-8", 5.0, 25.0, 0.5, 6.25),
    ("claude-opus-4-7", 5.0, 25.0, 0.5, 6.25),
    ("claude-opus-4-6", 5.0, 25.0, 0.5, 6.25),
    ("claude-sonnet-5-5", 2.0, 10.0, 0.20, 2.5),
    ("claude-sonnet-5", 2.0, 10.0, 0.20, 2.5),
    ("claude-sonnet-4-6", 3.0, 15.0, 0.3, 3.75),
    ("claude-haiku-5-5", 0.10, 0.50, 0.01, 0.125),
    ("claude-haiku-4-5", 1.0, 5.0, 0.1, 1.25),
];

fn base_id(model: &str) -> &str {
    model.split('[').next().unwrap_or(model).trim()
}

/// Context window in tokens. `[1m]` suffix -> 1M; any other `claude-*` -> 200K; else None.
pub fn context_window(requested_model: &str) -> Option<u64> {
    if requested_model.contains("[1m]") {
        Some(1_000_000)
    } else if requested_model.starts_with("claude-") {
        Some(200_000)
    } else {
        None
    }
}

/// Estimated cost in USD, or None for an unknown model. A `[1m]` suffix is ignored.
pub fn cost_usd(model: &str, u: &Usage) -> Option<f64> {
    let id = base_id(model);
    let &(_, inp, out, cr, cw) = PRICES.iter().find(|r| r.0 == id)?;
    let m = 1_000_000.0;
    Some(
        (u.input as f64 * inp
            + u.output as f64 * out
            + u.cache_read as f64 * cr
            + u.cache_creation as f64 * cw)
            / m,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_known_and_unknown() {
        let u = Usage { input: 1_000_000, output: 1_000_000, cache_read: 0, cache_creation: 0 };
        assert_eq!(cost_usd("claude-opus-5-5[1m]", &u), Some(24.0));
        assert_eq!(cost_usd("gpt-x", &u), None);
        assert_eq!(context_window("claude-opus-5-5"), Some(200_000));
        assert_eq!(context_window("foo"), None);
    }
}
