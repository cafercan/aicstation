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
    /// When the current busy stretch started (None when not busy).
    busy_since: Option<Instant>,
    agents_finished: u32,
}

#[derive(Default)]
pub struct Detector {
    /// None until the first snapshot has been observed.
    seen: Option<HashMap<String, Seen>>,
    last_alert: HashMap<String, Instant>,
}

impl Detector {
    /// Compares `sessions` with the previous snapshot and returns the alerts
    /// to show now. Never alerts on the first snapshot. `focused` is the
    /// session the user is looking at in a focused window: no alert for it.
    pub fn observe(
        &mut self,
        sessions: &[SessionCard],
        now: Instant,
        toggles: AlertToggles,
        focused: Option<&str>,
    ) -> Vec<Alert> {
        let first = self.seen.is_none();
        let prev = self.seen.take().unwrap_or_default();
        let mut next = HashMap::with_capacity(sessions.len());
        let mut out = Vec::new();
        for card in sessions {
            let ph = phase(&card.status);
            let old = prev.get(&card.session_id);
            let busy_since = match (ph, old) {
                (Phase::Busy, Some(o)) if o.phase == Phase::Busy => o.busy_since,
                (Phase::Busy, _) => Some(now),
                _ => None,
            };
            if let (false, Some(o)) = (first, old) {
                let kind = if ph == Phase::Waiting && o.phase != Phase::Waiting {
                    toggles.waiting.then_some(AlertKind::Waiting)
                } else if ph == Phase::Idle
                    && o.phase == Phase::Busy
                    && o.busy_since.is_some_and(|t| now.duration_since(t) >= MIN_BUSY)
                {
                    toggles.finished.then_some(AlertKind::Finished)
                } else if card.agents_finished > o.agents_finished {
                    toggles.subagent.then_some(AlertKind::Subagent)
                } else {
                    None
                };
                if let Some(kind) = kind {
                    let debounced = self
                        .last_alert
                        .get(&card.session_id)
                        .is_some_and(|t| now.duration_since(*t) < DEBOUNCE);
                    if !debounced && focused != Some(card.session_id.as_str()) {
                        self.last_alert.insert(card.session_id.clone(), now);
                        out.push(Alert {
                            kind,
                            session_id: card.session_id.clone(),
                            name: card.name.clone(),
                            waiting_for: card.waiting_for.clone().filter(|w| !w.trim().is_empty()),
                        });
                    }
                }
            }
            next.insert(
                card.session_id.clone(),
                Seen { phase: ph, busy_since, agents_finished: card.agents_finished },
            );
        }
        // Forget sessions that are gone.
        self.last_alert.retain(|id, t| next.contains_key(id) && now.duration_since(*t) < DEBOUNCE);
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
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], t0, ALL, None).is_empty());
        // Still waiting: no new transition.
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 1), ALL, None).is_empty());
    }

    #[test]
    fn entering_waiting_alerts_with_waiting_for() {
        let mut d = Detector::default();
        let t0 = Instant::now();
        d.observe(&[card(A, SessionStatus::Busy)], t0, ALL, None);
        let mut w = card(A, SessionStatus::Waiting);
        w.waiting_for = Some("Bash: cargo test".into());
        let got = d.observe(&[w], secs(t0, 1), ALL, None);
        assert_eq!(kinds(&got), [(AlertKind::Waiting, A)]);
        assert_eq!(got[0].name, "demo-0b2f");
        assert_eq!(got[0].waiting_for.as_deref(), Some("Bash: cargo test"));
    }

    #[test]
    fn finished_only_after_ten_busy_seconds() {
        let t0 = Instant::now();
        // Busy 9 s -> idle: no alert.
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Idle)], t0, ALL, None);
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 1), ALL, None);
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 5), ALL, None);
        assert!(d.observe(&[card(A, SessionStatus::Idle)], secs(t0, 10), ALL, None).is_empty());
        // Busy 10 s -> idle: alert. The busy stretch is measured from its start.
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 20), ALL, None);
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 25), ALL, None);
        let got = d.observe(&[card(A, SessionStatus::Idle)], secs(t0, 30), ALL, None);
        assert_eq!(kinds(&got), [(AlertKind::Finished, A)]);
    }

    #[test]
    fn shell_counts_as_busy() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy)], t0, ALL, None);
        d.observe(&[card(A, SessionStatus::Other("shell".into()))], secs(t0, 6), ALL, None);
        let got = d.observe(&[card(A, SessionStatus::Idle)], secs(t0, 12), ALL, None);
        assert_eq!(kinds(&got), [(AlertKind::Finished, A)]);
    }

    #[test]
    fn busy_at_startup_counts_from_the_first_snapshot() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy)], t0, ALL, None);
        let got = d.observe(&[card(A, SessionStatus::Idle)], secs(t0, 11), ALL, None);
        assert_eq!(kinds(&got), [(AlertKind::Finished, A)]);
    }

    #[test]
    fn debounce_one_alert_per_session_per_15_seconds() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy), card(B, SessionStatus::Busy)], t0, ALL, None);
        let got = d.observe(&[card(A, SessionStatus::Waiting), card(B, SessionStatus::Busy)], secs(t0, 1), ALL, None);
        assert_eq!(kinds(&got), [(AlertKind::Waiting, A)]);
        d.observe(&[card(A, SessionStatus::Busy), card(B, SessionStatus::Busy)], secs(t0, 2), ALL, None);
        // A again within 15 s: dropped; B is independent.
        let got = d.observe(&[card(A, SessionStatus::Waiting), card(B, SessionStatus::Waiting)], secs(t0, 10), ALL, None);
        assert_eq!(kinds(&got), [(AlertKind::Waiting, B)]);
        d.observe(&[card(A, SessionStatus::Busy), card(B, SessionStatus::Busy)], secs(t0, 15), ALL, None);
        let got = d.observe(&[card(A, SessionStatus::Waiting), card(B, SessionStatus::Busy)], secs(t0, 16), ALL, None);
        assert_eq!(kinds(&got), [(AlertKind::Waiting, A)], "15 s after the last alert");
    }

    #[test]
    fn focused_session_is_suppressed() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy), card(B, SessionStatus::Busy)], t0, ALL, None);
        let got = d.observe(&[card(A, SessionStatus::Waiting), card(B, SessionStatus::Waiting)], secs(t0, 1), ALL, Some(A));
        assert_eq!(kinds(&got), [(AlertKind::Waiting, B)]);
    }

    #[test]
    fn toggles_and_subagents() {
        let t0 = Instant::now();
        let off = AlertToggles { waiting: false, finished: true, subagent: false };
        let mut d = Detector::default();
        d.observe(&[card(A, SessionStatus::Busy)], t0, off, None);
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 1), off, None).is_empty());
        // Subagent finished: only with its toggle (default off).
        let mut c = card(A, SessionStatus::Waiting);
        c.agents_total = 2;
        c.agents_finished = 1;
        assert!(d.observe(&[c.clone()], secs(t0, 2), off, None).is_empty());
        c.agents_finished = 2;
        let got = d.observe(&[c], secs(t0, 3), ALL, None);
        assert_eq!(kinds(&got), [(AlertKind::Subagent, A)]);
    }

    #[test]
    fn new_and_vanished_sessions() {
        let t0 = Instant::now();
        let mut d = Detector::default();
        d.observe(&[], t0, ALL, None);
        // A session that appears already waiting has no transition to report.
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 1), ALL, None).is_empty());
        d.observe(&[card(A, SessionStatus::Busy)], secs(t0, 2), ALL, None);
        d.observe(&[], secs(t0, 3), ALL, None);
        // Back after vanishing: treated as new.
        assert!(d.observe(&[card(A, SessionStatus::Waiting)], secs(t0, 4), ALL, None).is_empty());
    }
}
