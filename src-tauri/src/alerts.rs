//! Notification triggers, decided on the snapshot thread by comparing
//! consecutive snapshots per session id (works while the window is hidden).
//! Pure: time comes in as an `Instant`, focus and toggles as arguments.
use station_core::monitor::SessionCard;
use station_core::registry::SessionStatus;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A session must have been busy at least this long for "finished".
pub const MIN_BUSY: Duration = Duration::from_secs(10);
/// At most one notification per session within this window.
pub const DEBOUNCE: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// The session entered "waiting" (onay bekliyor).
    Waiting,
    /// A busy session became ready after >= MIN_BUSY.
    Finished,
    /// One of the session's subagents finished.
    Subagent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub kind: AlertKind,
    pub session_id: String,
    pub name: String,
    pub waiting_for: Option<String>,
}

/// Which alerts are enabled (from the settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlertToggles {
    pub waiting: bool,
    pub finished: bool,
    pub subagent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Busy,
    Waiting,
    Idle,
    Other,
}

fn phase(s: &SessionStatus) -> Phase {
    match s {
        SessionStatus::Busy => Phase::Busy,
        // A shell command running inside the session counts as work.
        SessionStatus::Other(o) if o == "shell" => Phase::Busy,
        SessionStatus::Waiting => Phase::Waiting,
        SessionStatus::Idle => Phase::Idle,
        SessionStatus::Other(_) => Phase::Other,
    }
}

struct Seen {
    phase: Phase,
    /// When the current busy stretch started (None when not in one). A
    /// stretch survives "waiting" and unknown-status interludes, so
    /// "finished" measures the whole stretch, not only its last busy part.
    busy_since: Option<Instant>,
    agents_finished: u32,
}

#[derive(Default)]
pub struct Detector {
    /// None until the first snapshot has been observed.
    seen: Option<HashMap<String, Seen>>,
    last_alert: HashMap<String, Instant>,
    /// An alert that came inside the debounce window of its session, shown
    /// once the window has passed (at most one per session; a newer one
    /// replaces it).
    pending: HashMap<String, Alert>,
}

impl Detector {
    /// Compares `sessions` with the previous snapshot and returns the alerts
    /// to show now. Never alerts on the first snapshot. `focused` are the
    /// sessions the user is looking at in a focused window (the detail view
    /// or up to two visible split panes): no alert for them.
    pub fn observe(
        &mut self,
        sessions: &[SessionCard],
        now: Instant,
        toggles: AlertToggles,
        focused: &[&str],
    ) -> Vec<Alert> {
        let first = self.seen.is_none();
        let prev = self.seen.take().unwrap_or_default();
        let mut next = HashMap::with_capacity(sessions.len());
        let mut out = Vec::new();
        for card in sessions {
            let ph = phase(&card.status);
            let old = prev.get(&card.session_id);
            let busy_since = match ph {
                Phase::Busy => old.and_then(|o| o.busy_since).or(Some(now)),
                Phase::Waiting | Phase::Other => old.and_then(|o| o.busy_since),
                Phase::Idle => None,
            };
            let is_focused = focused.contains(&card.session_id.as_str());
            let in_window =
                |last: &HashMap<String, Instant>| last.get(&card.session_id).is_some_and(|t| now.duration_since(*t) < DEBOUNCE);
            if let (false, Some(o)) = (first, old) {
                let kind = if ph == Phase::Waiting && o.phase != Phase::Waiting {
                    toggles.waiting.then_some(AlertKind::Waiting)
                } else if ph == Phase::Idle
                    && matches!(o.phase, Phase::Busy | Phase::Other)
                    && o.busy_since.is_some_and(|t| now.duration_since(t) >= MIN_BUSY)
                {
                    toggles.finished.then_some(AlertKind::Finished)
                } else if card.agents_finished > o.agents_finished {
                    toggles.subagent.then_some(AlertKind::Subagent)
                } else {
                    None
                };
                if let Some(kind) = kind {
                    let alert = Alert {
                        kind,
                        session_id: card.session_id.clone(),
                        name: card.name.clone(),
                        waiting_for: card.waiting_for.clone().filter(|w| !w.trim().is_empty()),
                    };
                    if is_focused {
                        // Seen on screen: nothing to tell, also not later.
                        self.pending.remove(&card.session_id);
                    } else if in_window(&self.last_alert) {
                        self.pending.insert(card.session_id.clone(), alert);
                    } else {
                        self.pending.remove(&card.session_id);
                        self.last_alert.insert(card.session_id.clone(), now);
                        out.push(alert);
                    }
                }
            }
            // A deferred alert whose window has passed: shown if still current.
            if !in_window(&self.last_alert) {
                if let Some(p) = self.pending.remove(&card.session_id) {
                    let enabled = match p.kind {
                        AlertKind::Waiting => toggles.waiting && ph == Phase::Waiting,
                        AlertKind::Finished => toggles.finished,
                        AlertKind::Subagent => toggles.subagent,
                    };
                    if enabled && !is_focused {
                        self.last_alert.insert(card.session_id.clone(), now);
                        out.push(Alert { name: card.name.clone(), ..p });
                    }
                }
            } else if is_focused {
                self.pending.remove(&card.session_id);
            }
            next.insert(
                card.session_id.clone(),
                Seen { phase: ph, busy_since, agents_finished: card.agents_finished },
            );
        }
        // Forget sessions that are gone.
        self.last_alert.retain(|id, t| next.contains_key(id) && now.duration_since(*t) < DEBOUNCE);
        self.pending.retain(|id, _| next.contains_key(id));
        self.seen = Some(next);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use station_core::host::HostKind;

    const A: &str = "0b2f6a40-1111-4c3e-9d2a-5f1e2d3c4b5a";
    const B: &str = "7c9e1d20-2222-4a8b-8e3f-1a2b3c4d5e6f";
    const ALL: AlertToggles = AlertToggles { waiting: true, finished: true, subagent: true };

    fn card(id: &str, status: SessionStatus) -> SessionCard {
        SessionCard {
            session_id: id.into(),
            pid: 4242,
            parent_pid: None,
            name: format!("demo-{}", &id[..4]),
            cwd: r"C:\work\demo".into(),
            status,
            waiting_for: None,
            model: None,
            context_pct: None,
            cost_usd: 0.0,
            tokens: Default::default(),
            agents_total: 0,
            agents_finished: 0,
            tool_ok: 0,
            tool_err: 0,
            last_tool: None,
            started_at_ms: 0,
            last_activity_ms: 0,
            git_branch: None,
            host_kind: HostKind::Other,
        }
    }

    fn secs(t0: Instant, s: u64) -> Instant {
        t0 + Duration::from_secs(s)
    }

    fn kinds(v: &[Alert]) -> Vec<(AlertKind, &str)> {
        v.iter().map(|a| (a.kind, a.session_id.as_str())).collect()
    }

    #[test]
    fn nothing_on_the_first_snapshot() {
        let mut d = Detector::default();
        let t0 = Instant::now();
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], t0, ALL, &[]).is_empty());
        // Still waiting: no new transition.
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 1), ALL, &[]).is_empty());
    }

    #[test]
    fn entering_waiting_alerts_with_waiting_for() {
        let mut d = Detector::default();
        let t0 = Instant::now();
        d.observe(&[card(A, SessionStatus::Busy)], t0, ALL, &[]);
        let mut w = card(A, SessionStatus::Waiting);
        w.waiting_for = Some("Bash: cargo test".into());
        let got = d.observe(&[w], secs(t0, 1), ALL, &[]);
        assert_eq!(kinds(&got), [(AlertKind::Waiting, A)]);
        assert_eq!(got[0].name, "demo-0b2f");
        assert_eq!(got[0].waiting_for.as_deref(), Some("Bash: cargo test"));
    }

    #[test]
    fn finished_only_after_ten_busy_seconds() {
        let t0 = Instant::now();
        // Busy 9 s -> idle: no alert.
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Idle)], t0, ALL, &[]);
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 1), ALL, &[]);
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 5), ALL, &[]);
        assert!(d.observe(&[card(A, SessionStatus::Idle)], secs(t0, 10), ALL, &[]).is_empty());
        // Busy 10 s -> idle: alert. The busy stretch is measured from its start.
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 20), ALL, &[]);
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 25), ALL, &[]);
        let got = d.observe(&[card(A, SessionStatus::Idle)], secs(t0, 30), ALL, &[]);
        assert_eq!(kinds(&got), [(AlertKind::Finished, A)]);
    }

    #[test]
    fn shell_counts_as_busy() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy)], t0, ALL, &[]);
        d.observe(&[card(A, SessionStatus::Other("shell".into()))], secs(t0, 6), ALL, &[]);
        let got = d.observe(&[card(A, SessionStatus::Idle)], secs(t0, 12), ALL, &[]);
        assert_eq!(kinds(&got), [(AlertKind::Finished, A)]);
    }

    #[test]
    fn busy_at_startup_counts_from_the_first_snapshot() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy)], t0, ALL, &[]);
        let got = d.observe(&[card(A, SessionStatus::Idle)], secs(t0, 11), ALL, &[]);
        assert_eq!(kinds(&got), [(AlertKind::Finished, A)]);
    }

    #[test]
    fn debounce_one_alert_per_session_per_15_seconds() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy), card(B, SessionStatus::Busy)], t0, ALL, &[]);
        let got = d.observe(&[card(A, SessionStatus::Waiting), card(B, SessionStatus::Busy)], secs(t0, 1), ALL, &[]);
        assert_eq!(kinds(&got), [(AlertKind::Waiting, A)]);
        d.observe(&[card(A, SessionStatus::Busy), card(B, SessionStatus::Busy)], secs(t0, 2), ALL, &[]);
        // A again within 15 s: held back; B is independent.
        let got = d.observe(&[card(A, SessionStatus::Waiting), card(B, SessionStatus::Waiting)], secs(t0, 10), ALL, &[]);
        assert_eq!(kinds(&got), [(AlertKind::Waiting, B)]);
        // Still inside A's window: nothing yet.
        assert!(d.observe(&[card(A, SessionStatus::Waiting), card(B, SessionStatus::Waiting)], secs(t0, 12), ALL, &[]).is_empty());
        // The window has passed and A still waits: the deferred alert fires once.
        let got = d.observe(&[card(A, SessionStatus::Waiting), card(B, SessionStatus::Waiting)], secs(t0, 16), ALL, &[]);
        assert_eq!(kinds(&got), [(AlertKind::Waiting, A)], "15 s after the last alert");
        assert!(d.observe(&[card(A, SessionStatus::Waiting), card(B, SessionStatus::Waiting)], secs(t0, 40), ALL, &[]).is_empty());
    }

    #[test]
    fn deferred_alert_keeps_only_the_newest_and_drops_stale_waiting() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy)], t0, ALL, &[]);
        assert_eq!(kinds(&d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 1), ALL, &[])), [(AlertKind::Waiting, A)]);
        // Busy again, then waiting again inside the window: deferred.
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 2), ALL, &[]);
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 3), ALL, &[]).is_empty());
        // It is no longer waiting when the window passes: the stale alert is dropped.
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 4), ALL, &[]);
        assert!(d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 20), ALL, &[]).is_empty());

        // Two alerts inside one window: only the newest (finished) is kept.
        let mut d = Detector::default();
        d.observe(&[card(B, SessionStatus::Busy)], t0, ALL, &[]);
        assert_eq!(d.observe(&[card(B, SessionStatus::Waiting)], secs(t0, 1), ALL, &[]).len(), 1);
        d.observe(&[card(B, SessionStatus::Busy)], secs(t0, 2), ALL, &[]);
        assert!(d.observe(&[card(B, SessionStatus::Waiting)], secs(t0, 3), ALL, &[]).is_empty());
        d.observe(&[card(B, SessionStatus::Busy)], secs(t0, 4), ALL, &[]);
        assert!(d.observe(&[card(B, SessionStatus::Idle)], secs(t0, 12), ALL, &[]).is_empty());
        let got = d.observe(&[card(B, SessionStatus::Idle)], secs(t0, 17), ALL, &[]);
        assert_eq!(kinds(&got), [(AlertKind::Finished, B)]);
        assert!(d.observe(&[card(B, SessionStatus::Idle)], secs(t0, 40), ALL, &[]).is_empty());
    }

    #[test]
    fn deferred_alert_is_dropped_when_the_session_gets_focus_or_vanishes() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy)], t0, ALL, &[]);
        d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 1), ALL, &[]);
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 2), ALL, &[]);
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 3), ALL, &[]).is_empty());
        // The user looks at it inside the window.
        d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 5), ALL, &[A]);
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 20), ALL, &[]).is_empty());

        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy)], t0, ALL, &[]);
        d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 1), ALL, &[]);
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 2), ALL, &[]);
        d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 3), ALL, &[]);
        d.observe(&[], secs(t0, 4), ALL, &[]);
        d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 5), ALL, &[]);
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 20), ALL, &[]).is_empty());
    }

    #[test]
    fn busy_stretch_survives_a_waiting_interlude() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Idle)], t0, ALL, &[]);
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 1), ALL, &[]);
        // Approval asked after 4 s (alert shown), granted, 4 more busy seconds.
        assert_eq!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 5), ALL, &[]).len(), 1);
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 7), ALL, &[]);
        // Idle at 11 s: the stretch started at 1 s, so it counts as finished
        // (deferred: the waiting alert was 6 s ago).
        assert!(d.observe(&[card(A, SessionStatus::Idle)], secs(t0, 11), ALL, &[]).is_empty());
        let got = d.observe(&[card(A, SessionStatus::Idle)], secs(t0, 20), ALL, &[]);
        assert_eq!(kinds(&got), [(AlertKind::Finished, A)]);
        // Waiting -> idle (approval refused) is not "finished".
        let mut d = Detector::default();
        d.observe(&[card(B, SessionStatus::Busy)], t0, ALL, &[]);
        d.observe(&[card(B, SessionStatus::Waiting)], secs(t0, 20), ALL, &[]);
        assert!(d.observe(&[card(B, SessionStatus::Idle)], secs(t0, 40), ALL, &[]).is_empty());
    }

    #[test]
    fn busy_then_unknown_status_then_idle_is_finished() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy)], t0, ALL, &[]);
        assert!(d.observe(&[card(A, SessionStatus::Other("compacting".into()))], secs(t0, 8), ALL, &[]).is_empty());
        let got = d.observe(&[card(A, SessionStatus::Idle)], secs(t0, 12), ALL, &[]);
        assert_eq!(kinds(&got), [(AlertKind::Finished, A)]);
        // Unknown status straight from idle: no stretch, no alert.
        d.observe(&[card(A, SessionStatus::Other("x".into()))], secs(t0, 40), ALL, &[]);
        assert!(d.observe(&[card(A, SessionStatus::Idle)], secs(t0, 60), ALL, &[]).is_empty());
    }

    #[test]
    fn both_split_panes_are_suppressed() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy), card(B, SessionStatus::Busy)], t0, ALL, &[]);
        let got = d.observe(&[card(A, SessionStatus::Waiting), card(B, SessionStatus::Waiting)], secs(t0, 1), ALL, &[A, B]);
        assert!(got.is_empty());
    }

    #[test]
    fn focused_session_is_suppressed() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy), card(B, SessionStatus::Busy)], t0, ALL, &[]);
        let got = d.observe(&[card(A, SessionStatus::Waiting), card(B, SessionStatus::Waiting)], secs(t0, 1), ALL, &[A]);
        assert_eq!(kinds(&got), [(AlertKind::Waiting, B)]);
    }

    #[test]
    fn toggles_and_subagents() {
        let t0 = Instant::now();
        let off = AlertToggles { waiting: false, finished: true, subagent: false };
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy)], t0, off, &[]);
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 1), off, &[]).is_empty());
        // Subagent finished: only with its toggle (default off).
        let mut c = card(A, SessionStatus::Waiting);
        c.agents_total = 2;
        c.agents_finished = 1;
        assert!(d.observe(&[c.clone()], secs(t0, 2), off, &[]).is_empty());
        c.agents_finished = 2;
        let got = d.observe(&[c], secs(t0, 3), ALL, &[]);
        assert_eq!(kinds(&got), [(AlertKind::Subagent, A)]);
    }

    #[test]
    fn new_and_vanished_sessions() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[], t0, ALL, &[]);
        // A session that appears already waiting has no transition to report.
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 1), ALL, &[]).is_empty());
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 2), ALL, &[]);
        d.observe(&[], secs(t0, 3), ALL, &[]);
        // Back after vanishing: treated as new.
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 4), ALL, &[]).is_empty());
    }
}
