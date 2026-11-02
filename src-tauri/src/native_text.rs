//! Texts Rust shows itself (toasts, tray tooltip and menu), in the UI
//! language from the settings. This table is their only source; the UI's
//! dictionaries (ui/src/i18n) hold everything shown in the webview.
//! Counts use a "label: n" shape so no language needs plural forms.

pub struct Texts {
    /// `{name}` entered "waiting".
    pub waiting: &'static str,
    /// `{name}` finished its work.
    pub finished: &'static str,
    /// A subagent of `{name}` finished.
    pub subagent: &'static str,
    /// One-time note after the first close-to-tray.
    pub tray_notice: &'static str,
    /// Tray tooltip: `{open}` sessions, `{waiting}` of them waiting.
    pub tooltip: &'static str,
    pub show_hide: &'static str,
    pub dashboard: &'static str,
    pub quit: &'static str,
}

const TR: Texts = Texts {
    waiting: "{name} onay bekliyor",
    finished: "{name} tamamlandı",
    subagent: "{name}: bir alt ajan tamamlandı",
    tray_notice: "SessionStation tepside çalışmaya devam ediyor",
    tooltip: "SessionStation — {open} açık oturum, {waiting} onay bekliyor",
    show_hide: "Göster / Gizle",
    dashboard: "Dashboard",
    quit: "Çık",
};
const AZ: Texts = Texts {
    waiting: "{name} təsdiq gözləyir",
    finished: "{name} tamamlandı",
    subagent: "{name}: bir alt agent tamamlandı",
    tray_notice: "SessionStation sistem tepsisində işləməyə davam edir",
    tooltip: "SessionStation — {open} açıq sessiya, {waiting} təsdiq gözləyir",
    show_hide: "Göstər / Gizlət",
    dashboard: "Panel",
    quit: "Çıxış",
};
const EN: Texts = Texts {
    waiting: "{name} is awaiting approval",
    finished: "{name} finished",
    subagent: "{name}: a subagent finished",
    tray_notice: "SessionStation keeps running in the tray",
    tooltip: "SessionStation — open sessions: {open}, awaiting approval: {waiting}",
    show_hide: "Show / Hide",
    dashboard: "Dashboard",
    quit: "Quit",
};
const DE: Texts = Texts {
    waiting: "{name} wartet auf Freigabe",
    finished: "{name} ist fertig",
    subagent: "{name}: ein Subagent ist fertig",
    tray_notice: "SessionStation läuft im Infobereich weiter",
    tooltip: "SessionStation — offene Sitzungen: {open}, warten auf Freigabe: {waiting}",
    show_hide: "Anzeigen / Ausblenden",
    dashboard: "Übersicht",
    quit: "Beenden",
};
const RU: Texts = Texts {
    waiting: "{name} ждёт подтверждения",
    finished: "{name}: работа завершена",
    subagent: "{name}: субагент завершил работу",
    tray_notice: "SessionStation продолжает работать в области уведомлений",
    tooltip: "SessionStation — открытых сессий: {open}, ждут подтверждения: {waiting}",
    show_hide: "Показать / Скрыть",
    dashboard: "Панель",
    quit: "Выход",
};
const ES: Texts = Texts {
    waiting: "{name} está esperando aprobación",
    finished: "{name} ha terminado",
    subagent: "{name}: un subagente ha terminado",
    tray_notice: "SessionStation sigue en ejecución en la bandeja",
    tooltip: "SessionStation — sesiones abiertas: {open}, esperando aprobación: {waiting}",
    show_hide: "Mostrar / Ocultar",
    dashboard: "Panel",
    quit: "Salir",
};
const FR: Texts = Texts {
    waiting: "{name} est en attente d'approbation",
    finished: "{name} a terminé",
    subagent: "{name} : un sous-agent a terminé",
    tray_notice: "SessionStation continue de fonctionner dans la zone de notification",
    tooltip: "SessionStation — sessions ouvertes : {open}, en attente d'approbation : {waiting}",
    show_hide: "Afficher / Masquer",
    dashboard: "Tableau de bord",
    quit: "Quitter",
};
const ZH: Texts = Texts {
    waiting: "{name} 等待批准",
    finished: "{name} 已完成",
    subagent: "{name}：一个子智能体已完成",
    tray_notice: "SessionStation 仍在系统托盘中运行",
    tooltip: "SessionStation — 打开的会话：{open}，等待批准：{waiting}",
    show_hide: "显示 / 隐藏",
    dashboard: "总览",
    quit: "退出",
};

/// Texts for a language code; Turkish for anything unknown.
pub fn texts(lang: &str) -> &'static Texts {
    match lang {
        "az" => &AZ,
        "en" => &EN,
        "de" => &DE,
        "ru" => &RU,
        "es" => &ES,
        "fr" => &FR,
        "zh" => &ZH,
        _ => &TR,
    }
}

/// `template` with `{name}` replaced (a name containing "{open}" etc. is not
/// expanded again: there is one pass per placeholder over the template only).
pub fn with_name(template: &str, name: &str) -> String {
    template.replacen("{name}", name, 1)
}

/// Longest tray tooltip Windows shows (NOTIFYICONDATAW.szTip is 128 UTF-16
/// units including the terminator).
const MAX_TOOLTIP: usize = 127;

pub fn tooltip(lang: &str, open: usize, waiting: usize) -> String {
    let s = texts(lang)
        .tooltip
        .replace("{open}", &open.to_string())
        .replace("{waiting}", &waiting.to_string());
    cut_utf16(&s, MAX_TOOLTIP)
}

/// Menu label of a waiting session (names are capped for the menu width).
pub fn waiting_item(name: &str) -> String {
    let short: String = name.chars().take(40).collect();
    let more = if name.chars().nth(40).is_some() { "…" } else { "" };
    format!("⏳ {short}{more}")
}

/// `s` cut to at most `max` UTF-16 units, never inside a character.
fn cut_utf16(s: &str, max: usize) -> String {
    let mut n = 0;
    s.chars()
        .take_while(|c| {
            n += c.len_utf16();
            n <= max
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::LANGS;

    fn all() -> Vec<&'static Texts> {
        LANGS.iter().map(|l| texts(l)).collect()
    }

    #[test]
    fn every_language_has_every_text_with_its_placeholders() {
        for (lang, t) in LANGS.iter().zip(all()) {
            for (k, s, ph) in [
                ("waiting", t.waiting, &["{name}"][..]),
                ("finished", t.finished, &["{name}"]),
                ("subagent", t.subagent, &["{name}"]),
                ("tray_notice", t.tray_notice, &[]),
                ("tooltip", t.tooltip, &["{open}", "{waiting}"]),
                ("show_hide", t.show_hide, &[]),
                ("dashboard", t.dashboard, &[]),
                ("quit", t.quit, &[]),
            ] {
                assert!(!s.trim().is_empty(), "{lang}.{k}");
                for p in ph {
                    assert_eq!(s.matches(p).count(), 1, "{lang}.{k} {p}");
                }
                assert!(s.matches('{').count() == ph.len(), "{lang}.{k}: unexpected placeholder");
            }
            assert!(t.tooltip.starts_with("SessionStation"), "{lang}");
            assert!(t.tray_notice.contains("SessionStation"), "{lang}");
        }
    }

    #[test]
    fn unknown_language_falls_back_to_turkish() {
        assert_eq!(texts("xx").quit, "Çık");
        assert_eq!(texts("").waiting, TR.waiting);
    }

    #[test]
    fn tooltip_text() {
        assert_eq!(tooltip("tr", 3, 1), "SessionStation — 3 açık oturum, 1 onay bekliyor");
        assert_eq!(tooltip("en", 0, 0), "SessionStation — open sessions: 0, awaiting approval: 0");
        for l in LANGS {
            assert!(tooltip(l, 999, 999).encode_utf16().count() <= MAX_TOOLTIP, "{l}");
        }
    }

    #[test]
    fn name_is_inserted_once_and_never_expanded() {
        assert_eq!(with_name(texts("tr").waiting, "api"), "api onay bekliyor");
        assert_eq!(with_name(texts("tr").finished, "{name}{open}"), "{name}{open} tamamlandı");
    }

    #[test]
    fn utf16_cut_respects_characters() {
        assert_eq!(cut_utf16("ab😀c", 3), "ab");
        assert_eq!(cut_utf16("ab😀c", 4), "ab😀");
        assert_eq!(waiting_item("short"), "⏳ short");
        assert_eq!(waiting_item(&"x".repeat(41)), format!("⏳ {}…", "x".repeat(40)));
    }
}
