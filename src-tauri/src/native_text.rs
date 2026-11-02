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
    /// Message box of a second launch whose running instance did not answer.
    pub already_running: &'static str,
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
    already_running: "SessionStation zaten çalışıyor gibi görünüyor, ancak yanıt vermedi. Tepsideki simgesine bakın; açılmazsa Görev Yöneticisi'nden kapatıp yeniden başlatın.",
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
    already_running: "SessionStation artıq işləyir kimi görünür, lakin cavab vermədi. Sistem tepsisindəki ikonuna baxın; açılmazsa, Tapşırıq Menecerindən bağlayıb yenidən başladın.",
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
    already_running: "SessionStation already seems to be running but did not respond. Check its tray icon; if it does not open, end it in Task Manager and start it again.",
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
    already_running: "SessionStation scheint bereits zu laufen, hat aber nicht reagiert. Prüfen Sie das Symbol im Infobereich; öffnet es sich nicht, beenden Sie es im Task-Manager und starten Sie es neu.",
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
    already_running: "SessionStation, похоже, уже запущен, но не отвечает. Проверьте значок в области уведомлений; если окно не открывается, завершите процесс в диспетчере задач и запустите снова.",
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
    already_running: "SessionStation parece estar ya en ejecución, pero no respondió. Revise su icono en la bandeja; si no se abre, ciérrelo desde el Administrador de tareas y vuelva a iniciarlo.",
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
    already_running: "SessionStation semble déjà en cours d'exécution, mais n'a pas répondu. Vérifiez son icône dans la zone de notification ; s'il ne s'ouvre pas, arrêtez-le dans le Gestionnaire des tâches et relancez-le.",
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
    already_running: "SessionStation 似乎已在运行，但没有响应。请查看托盘图标；如果无法打开，请在任务管理器中结束它后重新启动。",
};

/// Text of the "already running but not responding" message box: in the
/// stored language when settings.json names one, otherwise English (the
/// Turkish default is for a configured app, not for a first-run message).
pub fn already_running(stored_lang: Option<&str>) -> &'static str {
    texts(stored_lang.unwrap_or("en")).already_running
}

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

/// Unicode general category Cf (format characters: bidi overrides and
/// isolates, zero-width characters, BOM, tag characters...). Hand-written
/// table (Unicode 15.1) so no crate is needed; it only has to be complete
/// enough to keep invisible or reordering characters out of native labels.
pub fn is_format_char(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD
            | 0x0600..=0x0605
            | 0x061C
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

/// Untrusted text (a session name) cleaned for a native label: control
/// characters (tab and newline included) and format characters dropped.
pub fn clean_label(s: &str) -> String {
    s.chars().filter(|&c| !c.is_control() && !is_format_char(c)).collect()
}

/// Menu label of a waiting session: the name cleaned, capped for the menu
/// width and with `&` doubled (a single `&` would mark a mnemonic and vanish).
pub fn waiting_item(name: &str) -> String {
    let name = clean_label(name);
    let short: String = name.chars().take(40).collect();
    let more = if name.chars().nth(40).is_some() { "…" } else { "" };
    format!("⏳ {}{more}", short.replace('&', "&&"))
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
                ("already_running", t.already_running, &[]),
            ] {
                assert!(!s.trim().is_empty(), "{lang}.{k}");
                for p in ph {
                    assert_eq!(s.matches(p).count(), 1, "{lang}.{k} {p}");
                }
                assert!(s.matches('{').count() == ph.len(), "{lang}.{k}: unexpected placeholder");
            }
            assert!(t.tooltip.starts_with("SessionStation"), "{lang}");
            assert!(t.tray_notice.contains("SessionStation"), "{lang}");
            assert!(t.already_running.starts_with("SessionStation"), "{lang}");
        }
    }

    #[test]
    fn already_running_uses_the_stored_language_or_english() {
        assert!(already_running(Some("tr")).starts_with("SessionStation zaten çalışıyor gibi görünüyor"));
        assert_eq!(already_running(Some("de")), DE.already_running);
        assert_eq!(already_running(None), EN.already_running);
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

    #[test]
    fn menu_labels_escape_ampersands_and_drop_tabs_controls_and_format_chars() {
        assert_eq!(waiting_item("R&D"), "⏳ R&&D");
        assert_eq!(waiting_item("a&&b"), "⏳ a&&&&b");
        // A tab would start the accelerator column; newlines and C0/C1 controls go too.
        assert_eq!(waiting_item("ad\tCtrl+Q"), "⏳ adCtrl+Q");
        assert_eq!(waiting_item("a\nb\r\u{1b}c\u{85}"), "⏳ abc");
        // Bidi overrides/isolates, zero-width chars, BOM and tag chars are dropped.
        assert_eq!(waiting_item("\u{202E}txt.exe\u{202C}"), "⏳ txt.exe");
        assert_eq!(waiting_item("a\u{2066}b\u{2069}\u{200B}c\u{FEFF}d\u{E0041}"), "⏳ abcd");
        // The 40-char cap counts the cleaned name, before `&` is doubled.
        let long = format!("{}&", "x".repeat(39));
        assert_eq!(waiting_item(&format!("\u{202E}{long}")), format!("⏳ {}&&", "x".repeat(39)));
        // Ordinary letters, accents, CJK and emoji stay.
        assert_eq!(clean_label("Çalışma 中文 😀"), "Çalışma 中文 😀");
        for c in ['\u{00AD}', '\u{061C}', '\u{200E}', '\u{2060}', '\u{FFF9}', '\u{E0001}'] {
            assert!(is_format_char(c), "{:04X}", c as u32);
        }
        for c in ['a', 'ı', '中', '😀', ' ', '\u{00A0}'] {
            assert!(!is_format_char(c), "{:04X}", c as u32);
        }
    }
}
