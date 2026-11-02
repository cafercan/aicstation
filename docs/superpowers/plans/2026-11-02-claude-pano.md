# claude-pano Uygulama Planı

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Hedef:** Makinedeki açık Claude Code oturumlarını kartlarda gösteren, oturum
ayrıntısı ve geçmiş sunan, içinde gerçek PowerShell terminalleri açabilen bir
Windows masaüstü uygulaması.

**Mimari:** İki Rust crate'inden oluşan bir Cargo workspace. `pano-core`
işletim sisteminden bağımsız, saf okuyucu kütüphane: Claude Code'un dosyalarını
okur ve model üretir. `claude-pano` Tauri 2 uygulaması: süreç sorgusu, PTY ve IPC
burada. Arayüz `ui/` klasöründe Vite + React + TypeScript ile yazılır, terminal
için `xterm.js` kullanılır. Platforma özgü kod `claude-pano/src/platform/`
altında toplanır.

**Teknoloji:**
- Rust 1.99 (edition 2021), Tauri 2, `portable-pty`, `notify`,
  `windows-sys`, `chrono`, `serde_json`
- Node 22, Vite, React 18, TypeScript, `@xterm/xterm`, `@xterm/addon-fit`,
  Vitest, Testing Library

**Spec:** `docs/superpowers/specs/2026-11-02-claude-pano-design.md`. Her görevden
önce okunur; bu plan spec'teki kararları tekrar etmez, uygular.

## Genel Kısıtlar

Her görevin gereksinimlerine bu bölüm de dahildir.

- Claude Code dosyalarına (`~/.claude/**`) hiçbir zaman yazılmaz. `~/.claude/sessions/*.key` dosyaları okunmaz, açılmaz.
- Dış ağ isteği yok. Tauri updater ve shell-open-url eklentileri eklenmez. CSP:
  `default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' data:`.
  Uzak font, CDN ya da script kullanılmaz.
- Ağa açık port yok. Arayüz ile çekirdek arasındaki iletişim yalnızca Tauri IPC (komutlar ve `Channel`) üzerinden olur.
- Claude dizini: `CLAUDE_CONFIG_DIR` tanımlıysa o, değilse `%USERPROFILE%\.claude`.
- Arayüz metinleri Türkçe.
- Kayıt dosyaları akış halinde okunur. Hiçbir dosya tamamen belleğe alınmaz.
- Bilinmeyen alan, satır türü ya da `status` değeri gelirse çökme olmaz: atlanır ya da olduğu gibi gösterilir.
- Kimlikler:
  - Tauri `identifier`: `com.cafercan.claudepano`
  - Ürün adı: `Claude Pano`
- Commit mesajlarına AI imzası (Co-Authored-By vb.) eklenmez.
- Repo şu an private ama sonradan public yapılacak:
  - Koda, testlere ve fixture'lara gerçek oturum kimliği, kişisel yol, prompt
    metni ya da e-posta girmez. Fixture'lar elle yazılmış uydurma verilerdir.
  - Commit e-postası `10761599+cafercan@users.noreply.github.com` olarak kalır.
- Arayüzden gelen her IPC argümanı güvenilmez kabul edilir ve doğrulanır
  (bkz. Görev 5, 9, 10).

## İnceleme Odağı

Spec'in açıkça söylemediği ama kullanıcıyı en çok vurabilecek beş durum. Her
birinin testi, ilgili görevin içinde.

1. **Yarım satır:** Claude Code bir satırı yazarken okunursa, son satır yarım
   kalabilir. Bu satır kaybolmamalı ve çökmeye yol açmamalı; bir sonraki okumada
   tamamlanmış haliyle işlenmeli. Test: Görev 2, `partial_line_is_kept_for_next_read`.
2. **Dosya küçülmesi:** Kayıt dosyası yeniden yazılır ya da kısalırsa (dosya
   boyutu < kayıtlı konum), okuyucu baştan okumalı ve istatistikleri
   sıfırlamalı. Test: Görev 2, `shrunk_file_resets_offset`.
3. **Kayıt klasöründe beklenmedik dosyalar:** `sessions/` klasöründe yarım
   yazılmış JSON, `.key` dosyası ya da okuma sırasında silinen bir dosya
   olabilir. Bunlar sessizce atlanmalı. Test: Görev 4,
   `registry_skips_key_bad_and_vanished_files`.
4. **Bellek sınırı:** 429 MB'lık bir kayıt, bellekte yalnızca son 50 tool
   çağrısını tutmalı. Test: Görev 3, `tool_call_history_is_capped_at_50`.
5. **Türkçe karakterli ve boşluklu yollar:** Örneğin `D:\Workspaces\AI\SeyahatBütçe`
   doğru bulunmalı ve doğru gösterilmeli; "Klasörü aç" bozulmamalı. Test:
   Görev 5, `finds_transcript_under_non_ascii_project_dir`.

---

## Dosya Yapısı

```
Cargo.toml                      workspace: core, src-tauri
core/                           crate pano-core (OS'tan bağımsız)
  src/lib.rs                    modül dışa aktarımı
  src/model.rs                  ortak tipler (Usage, ToolCall, SessionStatus …)
  src/transcript.rs             JSONL satır ayrıştırma + artımlı okuyucu
  src/stats.rs                  SessionStats birikimi (dedupe, son 50 tool)
  src/pricing.rs                fiyat tablosu + bağlam penceresi
  src/registry.rs               sessions/*.json → LiveSession, ProcessProbe trait
  src/locate.rs                 sessionId → kayıt yolu, subagents klasörü
  src/agents.rs                 meta.json → ajan ağacı, çalışıyor kuralı
  src/monitor.rs                canlı Snapshot üretimi + sıralama
  src/history.rs                arka plan dizinleyici + önbellek + günlük maliyet
  tests/fixtures/…              küçük örnek dosyalar
src-tauri/                      crate claude-pano
  src/main.rs                   Tauri kurulumu, komut kaydı, arka plan döngüsü
  src/commands.rs               get_snapshot, get_session_detail, get_history, open_folder
  src/platform/mod.rs           platform seçimi
  src/platform/windows.rs       WinProbe: GetProcessTimes, üst PID
  src/pty.rs                    PtyManager (portable-pty)
  tauri.conf.json, capabilities/default.json
  tests/config_guard.rs         CSP/eklenti koruma testi
ui/                             Vite + React + TS
  src/api.ts                    IPC sarmalayıcı + TS tipleri
  src/App.tsx                   düzen: Sidebar + içerik + StatusBar
  src/components/Sidebar.tsx, StatusBar.tsx, SessionCard.tsx, KpiRow.tsx
  src/pages/Dashboard.tsx, SessionDetail.tsx, History.tsx, TerminalView.tsx
  src/terminal/theme.ts         Ubuntu şeması
  src/format.ts                 token/maliyet/süre biçimleme
  src/**/*.test.tsx
```

---

### Görev 1: İskelet ve güvenlik koruması

**Dosyalar:**
- Oluştur: `Cargo.toml`, `core/Cargo.toml`, `core/src/lib.rs`,
  `src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json,src/main.rs}`,
  `src-tauri/tests/config_guard.rs`, `ui/` (Vite React-TS şablonu), kökte
  `package.json` (yalnızca `@tauri-apps/cli` ve betikler)
- Değiştir: `.gitignore` (`ui/node_modules/`, `src-tauri/gen/` eklenir)

**Arayüzler:**
- Üretir:
  - `cargo test --workspace`, `npm --prefix ui test` ve `npm run tauri dev`
    çalışır.
  - Ön yüz `ui/dist`'ten yüklenir, geliştirmede `http://localhost:1420`
    kullanılır.

- [ ] **Adım 1:** Workspace'i `c-launcher` düzeniyle kur.
  - Release profili aynı: `lto=true`, `codegen-units=1`, `strip=true`,
    `opt-level="s"`.
  - `tauri.conf.json`: identifier, productName, Genel Kısıtlar'daki CSP, pencere
    boyutu 1280×800, `frontendDist: "../ui/dist"`,
    `devUrl: "http://localhost:1420"`, `beforeDevCommand` /
    `beforeBuildCommand` ile `ui` derlemesi.
  - Capabilities yalnızca `core:default`.
- [ ] **Adım 2: Koruma testini yaz**: `src-tauri/tests/config_guard.rs`

```rust
#[test] fn csp_has_no_remote_origins() {
    // tauri.conf.json'ı oku; app.security.csp içinde "http://" ve "https://" yalnızca
    // "http://ipc.localhost" olarak geçebilir.
}
#[test] fn no_updater_or_network_plugins() {
    // Cargo.toml bağımlılıklarında "tauri-plugin-updater", "tauri-plugin-http",
    // "tauri-plugin-shell", "reqwest" yok; tauri.conf.json'da "plugins.updater" yok.
}
```
- [ ] **Adım 3:** `cargo test -p claude-pano --test config_guard` çalıştır. Beklenen: 2 test PASS.
- [ ] **Adım 4:** `npm run tauri dev` çalıştır. Beklenen: boş bir "Claude Pano" penceresi açılır.
- [ ] **Adım 5:** Commit: `chore: scaffold tauri workspace with network guard`

### Görev 2: Kayıt satırı ayrıştırma ve artımlı okuyucu

**Dosyalar:**
- Oluştur: `core/src/model.rs`, `core/src/transcript.rs`
- Test: `core/src/transcript.rs` içindeki `#[cfg(test)]` bloğu ve `core/tests/fixtures/transcript_basic.jsonl`

**Arayüzler:**
- Üretir (`model.rs`):

```rust
pub struct Usage { pub input: u64, pub output: u64, pub cache_read: u64, pub cache_creation: u64 }
pub enum ToolState { Running, Ok, Error }
pub struct ToolCall { pub id: String, pub name: String, pub target: String,
                      pub at: chrono::DateTime<chrono::Utc>, pub state: ToolState }
```
- Üretir (`transcript.rs`):

```rust
pub enum Entry {
    Assistant { message_id: String, model: Option<String>, requested_model: Option<String>,
                usage: Option<Usage>, tool_uses: Vec<ToolCall>, at: DateTime<Utc> },
    ToolResult { tool_use_id: String, is_error: bool, at: DateTime<Utc> },
    Title(String),
    Meta { cwd: Option<String>, git_branch: Option<String>, at: Option<DateTime<Utc>> },
}
pub fn parse_line(line: &str) -> Vec<Entry>;          // bozuk/bilinmeyen satır → boş vec
pub struct TranscriptReader { /* path, offset, carry: Vec<u8> */ }
impl TranscriptReader {
    pub fn new(path: PathBuf) -> Self;
    pub fn read_new(&mut self) -> std::io::Result<ReadOutcome>;
}
pub struct ReadOutcome { pub entries: Vec<Entry>, pub reset: bool }
```

- [ ] **Adım 1: Başarısız testleri yaz**

```rust
#[test] fn parses_assistant_with_usage_and_tool_use() // model, requested_model "claude-opus-5-5[1m]", usage alanları, tool_uses[0].name=="Read"
#[test] fn parses_tool_result_error_flag()             // user satırı, tool_result is_error:true → Entry::ToolResult{is_error:true}
#[test] fn title_from_custom_title()                   // {"type":"custom-title","customTitle":"Dell"} → Title("Dell")
#[test] fn unknown_and_broken_lines_yield_nothing()    // "{not json", {"type":"api-request-blob"} → []
#[test] fn tool_target_is_short()                      // Read→file_path dosya adı, Bash→command ilk 60 karakter, Grep→pattern
#[test] fn partial_line_is_kept_for_next_read()        // "…}\n{\"type\":\"assi" yaz → 1 entry; satırı tamamla → sonraki read_new 1 entry
#[test] fn shrunk_file_resets_offset()                 // oku, dosyayı daha kısa içerikle yeniden yaz → reset==true, entries baştan
#[test] fn reads_in_chunks_without_loading_file()      // 5 MB'lık üretilmiş dosya; okuma 64 KiB tamponla yapılır, sonuç doğru sayıda entry
```
- [ ] **Adım 2:** `cargo test -p pano-core transcript` çalıştır. Beklenen: FAIL (tanımsız).
- [ ] **Adım 3:** Uygula.
  - `BufReader` + 64 KiB tampon kullanılır. Son `\n`'den sonraki kısım
    `carry` içinde tutulur.
  - Dosya boyutu `offset`'ten küçükse `offset=0` yapılır ve `reset=true`
    döner.
  - Tek bir `assistant` satırı birden çok `tool_use` bloğu içerebilir.
  - `user` satırının `content` alanı string ya da dizi olabilir.
- [ ] **Adım 4:** Testleri çalıştır. Beklenen: PASS.
- [ ] **Adım 5:** Commit: `feat(core): incremental transcript reader`

### Görev 3: Oturum istatistikleri, fiyat ve bağlam

**Dosyalar:** Oluştur: `core/src/stats.rs`, `core/src/pricing.rs`

**Arayüzler:**
- Tüketir: `Entry`, `Usage`, `ToolCall` (Görev 2)
- Üretir:

```rust
pub struct SessionStats {
    pub title: Option<String>, pub cwd: Option<String>, pub git_branch: Option<String>,
    pub model: Option<String>, pub requested_model: Option<String>,
    pub tokens: Usage,                     // message.id ile tekilleştirilmiş toplam
    pub last_usage: Option<Usage>,
    pub cost_usd: f64,
    pub cost_by_day: BTreeMap<chrono::NaiveDate, f64>, // yerel tarih
    pub tool_calls: VecDeque<ToolCall>,    // en fazla 50
    pub tool_call_count: u64,
    pub first_at: Option<DateTime<Utc>>, pub last_at: Option<DateTime<Utc>>,
}
impl SessionStats { pub fn apply(&mut self, e: &Entry); pub fn context_pct(&self) -> Option<f32>; }
// pricing.rs
pub fn context_window(requested_model: &str) -> Option<u64>;   // "[1m]" ekli → 1_000_000; tanınan claude-* → 200_000; diğer → None
pub fn cost_usd(model: &str, u: &Usage) -> Option<f64>;         // bilinmeyen model → None
```

- [ ] **Adım 1: Başarısız testleri yaz**

```rust
#[test] fn usage_is_deduplicated_by_message_id()   // aynı message.id'li 3 satır → tokens bir kez sayılır
#[test] fn context_pct_uses_requested_model_1m()   // requested "claude-opus-5-5[1m]", last_usage toplam 200_000 → 20.0
#[test] fn context_pct_unknown_model_is_none()
#[test] fn tool_result_updates_matching_call()     // ToolUse id=A sonra ToolResult A is_error → state Error
#[test] fn tool_call_history_is_capped_at_50()     // 120 tool_use → len 50, en eskisi #71, tool_call_count 120
#[test] fn cost_split_by_local_day()               // 23:50 ve 00:10 (yerel) iki yanıt → iki gün anahtarı
```
- [ ] **Adım 2:** `cargo test -p pano-core stats` çalıştır. Beklenen: FAIL.
- [ ] **Adım 3:** Uygula.
  - Görülen `message.id`'ler bir `HashSet` içinde tutulur.
  - Fiyat tablosu `pricing.rs` içinde tek bir `const` dizidir. Güncel değerler
    `claude-api` skill'inden alınır. Kaynağı ve tarihi yorum olarak yazılır.
- [ ] **Adım 4:** Testleri çalıştır. Beklenen: PASS.
- [ ] **Adım 5:** Commit: `feat(core): session stats with dedupe, pricing, context`

### Görev 4: Canlı oturum kaydı

**Dosyalar:** Oluştur: `core/src/registry.rs`

**Arayüzler:**
- Üretir:

```rust
pub enum SessionStatus { Busy, Waiting, Idle, Other(String) }
pub struct LiveSession { pub pid: u32, pub session_id: String, pub cwd: String, pub name: Option<String>,
                         pub status: SessionStatus, pub waiting_for: Option<String>,
                         pub started_at_ms: u64, pub status_updated_at_ms: u64, pub proc_start: u64 }
pub trait ProcessProbe { fn start_filetime(&self, pid: u32) -> Option<u64>; fn parent_pid(&self, pid: u32) -> Option<u32>; }
pub fn read_live_sessions(sessions_dir: &Path, probe: &dyn ProcessProbe) -> Vec<LiveSession>;
```

- [ ] **Adım 1: Başarısız testleri yaz.** Testlerde sahte bir `FakeProbe` (`HashMap` tabanlı) kullanılır.

```rust
#[test] fn reads_live_session_fields()                  // spec 5.1 örneği → alanlar; "busy"→Busy, "waiting"+waitingFor
#[test] fn dead_pid_is_excluded()                       // probe None döner → listede yok
#[test] fn reused_pid_is_excluded()                     // probe farklı filetime döner → listede yok
#[test] fn unknown_status_is_other()                    // "compacting" → Other("compacting")
#[test] fn registry_skips_key_bad_and_vanished_files()  // x.key, bozuk.json, liste sonrası silinen dosya → panik yok, yalnızca geçerli olan
```
- [ ] **Adım 2:** Testleri çalıştır. Beklenen: FAIL.
- [ ] **Adım 3:** Uygula.
  - Yalnızca `*.json` uzantılı dosyalar açılır.
  - `procStart` metin olarak gelir ve `u64`'e çevrilir.
- [ ] **Adım 4:** Testleri çalıştır. Beklenen: PASS.
- [ ] **Adım 5:** Commit: `feat(core): live session registry with pid-reuse guard`

### Görev 5: Kayıt dosyası bulma

**Dosyalar:** Oluştur: `core/src/locate.rs`

**Arayüzler:**

```rust
pub struct Locator { /* projects_dir, cache: HashMap<String, PathBuf> */ }
impl Locator {
    pub fn new(projects_dir: PathBuf) -> Self;
    pub fn transcript(&mut self, session_id: &str) -> Option<PathBuf>;   // <proj>/<id>.jsonl, tüm alt klasörlerde arar, sonucu önbelleğe alır
    pub fn subagents_dir(&mut self, session_id: &str) -> Option<PathBuf>; // <proj>/<id>/subagents
    pub fn all_transcripts(&self) -> Vec<(PathBuf, std::fs::Metadata)>;   // Geçmiş için; yalnızca üst düzey *.jsonl
}
pub fn claude_dir() -> PathBuf;   // CLAUDE_CONFIG_DIR ya da ~/.claude
```

- [ ] **Adım 1: Başarısız testleri yaz**

```rust
#[test] fn finds_transcript_in_any_project_dir()             // "d--x" ve "D--y" klasörleri; id y'de → bulunur
#[test] fn finds_transcript_under_non_ascii_project_dir()    // klasör "D--Workspaces-AI-SeyahatB-t-e"; içerikteki cwd "…SeyahatBütçe" korunur
#[test] fn cached_path_invalidated_when_missing()            // dosya silinir → yeniden arar, None
#[test] fn claude_dir_respects_env()                         // CLAUDE_CONFIG_DIR=X → X (test içinde env ayarlanır/geri alınır)
#[test] fn rejects_non_uuid_session_id()                     // "../../x", "a/b", "" → None, dosya sistemine dokunulmaz
```

`session_id` arayüzden gelir ve yol kurmak için kullanılır. Bu yüzden
`transcript` ve `subagents_dir`, yalnızca UUID biçimindeki kimlikleri
(`[0-9a-f-]{36}`) kabul eder. Bu kontrol yol geçişi (path traversal) açığını
engeller.
- [ ] **Adım 2–4:** Testleri FAIL gör, uygula, PASS gör.
- [ ] **Adım 5:** Commit: `feat(core): transcript locator`

### Görev 6: Ajan ağacı

**Dosyalar:**
- Oluştur: `core/src/agents.rs`
- Fixture: `core/tests/fixtures/subagents/` (bkz. Adım 1)

**Arayüzler:**
- Tüketir: `TranscriptReader`, `SessionStats` (Görev 2–3)
- Üretir:

```rust
pub enum AgentState { Running, Finished }
pub struct AgentNode { pub id: String, pub kind: AgentKind, pub agent_type: Option<String>,
                       pub description: Option<String>, pub state: AgentState,
                       pub stats: SessionStats, pub children: Vec<AgentNode> }
pub enum AgentKind { Agent, Workflow { id: String }, Phase { name: String } }
pub struct AgentTree { pub roots: Vec<AgentNode>, pub total: u32, pub finished: u32 }
pub fn build_tree(subagents_dir: &Path, session_live: bool, now: SystemTime) -> AgentTree;
```

- [ ] **Adım 1: Başarısız testleri yaz.** Fixture olarak üç ajan kullanılır:
  `a1` (derinlik 1, `finished:true`), `a2` (derinlik 1, `finished` alanı yok),
  `a3` (`parentAgentId: a2`). Ayrıca `workflows/wf_x/` altında `workflowPhase`
  değerleri "Review" ve "Verify" olan iki ajan bulunur.

```rust
#[test] fn depth2_agent_nests_under_parent()        // a3, a2.children içinde
#[test] fn workflow_agents_grouped_by_phase()       // Workflow{wf_x} → Phase{Review}, Phase{Verify}
#[test] fn counts_include_workflow_agents()         // total 5
#[test] fn finished_true_is_finished()
#[test] fn missing_finished_recent_mtime_is_running()   // session_live, mtime now-30s → Running
#[test] fn missing_finished_stale_and_resolved_is_finished() // mtime now-5dk, son tool_result var → Finished
#[test] fn missing_finished_stale_but_open_tool_is_running() // mtime eski, son tool_use sonuçsuz → Running
#[test] fn closed_session_all_finished()            // session_live=false → finished == total
#[test] fn model_comes_from_agent_transcript()      // meta.model "inherit", jsonl model "claude-sonnet-5-5" → stats.model
```
- [ ] **Adım 2–4:** Testleri FAIL gör, uygula, PASS gör. Her ajanın `.jsonl` dosyası için ayrı bir `TranscriptReader` kullanılır. 2 dakikalık eşik `const RUNNING_WINDOW: Duration` olarak tanımlanır.
- [ ] **Adım 5:** Commit: `feat(core): subagent tree with workflow grouping`

### Görev 7: Canlı izleyici

**Dosyalar:** Oluştur: `core/src/monitor.rs`

**Arayüzler:**
- Tüketir: Görev 2–6'da üretilen her şey.
- Üretir (tümü `serde::Serialize`, alan adları camelCase):

```rust
pub struct SessionCard { pub session_id: String, pub pid: u32, pub parent_pid: Option<u32>,
    pub name: String, pub cwd: String, pub status: SessionStatus, pub waiting_for: Option<String>,
    pub model: Option<String>, pub context_pct: Option<f32>, pub cost_usd: f64,
    pub agents_total: u32, pub agents_finished: u32, pub last_tool: Option<ToolCall>,
    pub started_at_ms: u64, pub last_activity_ms: u64, pub git_branch: Option<String> }
pub struct Snapshot { pub sessions: Vec<SessionCard>, pub running_agents: u32, pub waiting_count: u32 }
pub struct SessionDetail { pub card: SessionCard, pub tokens: Usage, pub tool_call_count: u64,
                           pub tree: AgentTree, pub main_tool_calls: Vec<ToolCall> }
pub struct Monitor { /* claude_dir, Locator, HashMap<session_id, (TranscriptReader, SessionStats)> */ }
impl Monitor {
    pub fn new(claude_dir: PathBuf) -> Self;
    pub fn refresh(&mut self, probe: &dyn ProcessProbe, now: SystemTime) -> Snapshot;
    pub fn detail(&mut self, session_id: &str, probe: &dyn ProcessProbe, now: SystemTime) -> Option<SessionDetail>;
}
```

- [ ] **Adım 1: Başarısız testleri yaz.** Testlerde geçici bir `claude_dir` (sessions + projects) ve `FakeProbe` kullanılır.

```rust
#[test] fn snapshot_lists_only_live_sessions()
#[test] fn sorted_waiting_then_busy_then_idle_then_recent() // spec 6.2 sıralaması
#[test] fn refresh_is_incremental()                   // satır ekle → ikinci refresh yalnızca yeni satırı okur (offset kontrolü)
#[test] fn ended_session_state_dropped()              // kayıt dosyası silinir → bellekteki reader kaldırılır
#[test] fn session_cost_includes_subagents()
#[test] fn name_falls_back_to_cwd_folder()            // name ve custom-title yok → cwd son bileşeni
#[test] fn detail_for_closed_session_from_history()   // canlı değil ama kayıt var → tüm ajanlar Finished
```
- [ ] **Adım 2–4:** Testleri FAIL gör, uygula, PASS gör.
- [ ] **Adım 5:** Commit: `feat(core): live monitor snapshot`

### Görev 8: Geçmiş dizinleyicisi

**Dosyalar:** Oluştur: `core/src/history.rs`

**Arayüzler:**

```rust
pub struct HistoryRow { pub session_id: String, pub path: PathBuf, pub modified_ms: u64, pub size: u64,
    pub name: String, pub cwd: Option<String>,
    pub summary: Option<HistorySummary> }          // None → arayüzde "?"
pub struct HistorySummary { pub model: Option<String>, pub first_at_ms: u64, pub last_at_ms: u64,
    pub agents_total: u32, pub cost_usd: f64, pub cost_by_day: BTreeMap<NaiveDate, f64> }
pub struct HistoryIndex { /* cache_path, rows, cache: HashMap<PathBuf, (size, mtime, HistorySummary)> */ }
impl HistoryIndex {
    pub fn load(cache_path: PathBuf, locator: &Locator) -> Self;  // önbellek + dosya listesi; özetsiz satırlar hemen
    pub fn step(&mut self, budget: Duration) -> bool;             // en yeni özetsiz dosyadan başlar; iş kaldıysa true
    pub fn save(&self) -> std::io::Result<()>;
    pub fn rows(&self, live_ids: &HashSet<String>) -> Vec<&HistoryRow>; // canlı olanlar hariç, yeniden eskiye
    pub fn cost_on(&self, day: NaiveDate) -> (f64, bool);          // (toplam, dizinleme bitti mi)
}
```

- [ ] **Adım 1: Başarısız testleri yaz**

```rust
#[test] fn rows_available_before_indexing()          // load sonrası summary None, name/cwd/tarih dolu
#[test] fn step_indexes_newest_first()
#[test] fn cache_skips_unchanged_files()             // save→load: aynı size/mtime yeniden okunmaz
#[test] fn changed_file_reindexed()
#[test] fn live_sessions_excluded_from_rows()
#[test] fn cost_on_today_sums_across_sessions_and_reports_incomplete() // bir dosya özetsizken bool=false
```
- [ ] **Adım 2–4:** Testleri FAIL gör, uygula, PASS gör.
  - Önbellek, Tauri'nin `app_data_dir` klasöründe `history-cache.json`
    dosyasıdır. Yol çağıran taraftan verilir.
  - Bir dosyanın ajan sayısı `subagents/` klasöründen sayılır. Ajan
    kayıtlarının maliyeti oturumun maliyetine eklenir.
- [ ] **Adım 5:** Commit: `feat(core): background history index with cache`

### Görev 9: Windows süreç sorgusu ve IPC komutları

**Dosyalar:**
- Oluştur: `src-tauri/src/platform/{mod.rs,windows.rs}`, `src-tauri/src/commands.rs`
- Değiştir: `src-tauri/src/main.rs`

**Arayüzler:**
- Tüketir: `Monitor`, `HistoryIndex`, `ProcessProbe`, `claude_dir()`
- Üretir:
  - Tauri komutları:
    - `get_snapshot() -> Snapshot`
    - `get_session_detail(session_id: String) -> Option<SessionDetail>`
    - `get_history() -> HistoryView { rows: Vec<HistoryRow>, today_cost: f64, today_complete: bool, indexing: bool }`
    - `open_folder(path: String) -> Result<(), String>`
  - Olay: Arka plan iş parçacığı her 1 saniyede ve `notify` bir değişiklik
    bildirdiğinde (en fazla 4 Hz) `"snapshot"` olayını `Snapshot` ile yayar.
    Boşta kaldığında `HistoryIndex::step(50ms)` çalıştırır ve her 30 saniyede
    önbelleği kaydeder.

- [ ] **Adım 1: Başarısız testleri yaz**: `platform/windows.rs` içinde

```rust
#[test] fn own_process_start_time_is_some()   // probe.start_filetime(std::process::id()).is_some()
#[test] fn own_parent_pid_is_some()
#[test] fn nonexistent_pid_is_none()          // pid 0xFFFF_FFF0 → None
```
- [ ] **Adım 2–4:** Testleri FAIL gör, uygula, PASS gör.
  - `WinProbe` şunlarla yazılır: `windows-sys`, `OpenProcess`
    (`PROCESS_QUERY_LIMITED_INFORMATION`), `GetProcessTimes` (creation
    FILETIME → `u64`) ve Toolhelp32 snapshot (üst PID).
  - Linux için `platform/mod.rs` içinde yalnızca `unimplemented!` içermeyen
    boş bir iskelet bırakılır; derlenmesi `cfg(windows)` ile sınırlanır.
  - `open_folder`, `explorer.exe`'yi tek argümanla başlatır ve yolu kabuktan
    geçirmez. Yalnızca o anda bilinen bir oturumun `cwd` değerini kabul eder
    (canlı ya da Geçmiş'te). Yol, var olan bir **klasör** olmalıdır. Aksi
    halde hata döner, çünkü `explorer.exe` bir `.exe` yolu verilirse onu
    çalıştırır. Testler: `open_folder_rejects_unknown_path`,
    `open_folder_rejects_file_path`.
- [ ] **Adım 5: Elle doğrula.** `npm run tauri dev` çalıştır. Geliştirici
  araçlarında `await __TAURI__.core.invoke('get_snapshot')` komutu, şu an açık
  olan Claude oturumlarını listeler. Sayısı `tasklist | findstr claude.exe`
  çıktısıyla aynıdır.
- [ ] **Adım 6:** Commit: `feat(app): process probe, snapshot events, commands`

### Görev 10: PTY yöneticisi

**Dosyalar:**
- Oluştur: `src-tauri/src/pty.rs`
- Değiştir: `src-tauri/src/main.rs`, `src-tauri/src/commands.rs`

**Arayüzler:**
- Üretir (Tauri komutları):
  - `pty_spawn(cols: u16, rows: u16, on_output: Channel<Vec<u8>>, on_exit: Channel<i32>) -> PtyInfo { id: u32, pid: u32 }`
  - `pty_write(id: u32, data: String)`
  - `pty_resize(id: u32, cols: u16, rows: u16)`
  - `pty_kill(id: u32)`
  - `pty_list() -> Vec<PtyInfo>`
- Davranış:
  - Varsayılan kabuk `powershell.exe -NoLogo`, profil yüklenir.
  - Başlangıç klasörü ev dizinidir.
  - Ortama `TERM=xterm-256color` ve `COLORTERM=truecolor` eklenir.

- [ ] **Adım 1: Başarısız testi yaz**

```rust
#[test] fn spawn_echo_and_exit() // PtyManager::spawn_cmd("cmd.exe", &["/c","echo pano-ok"]) → çıktıda "pano-ok", çıkış kodu 0
#[test] fn kill_removes_from_list()
#[test] fn unknown_id_is_error()   // pty_write/resize/kill bilinmeyen id → Err, panik yok
```

Arayüz yalnızca `powershell.exe` başlatabilir. `pty_spawn` dışarıdan program
adı ya da argüman almaz. `spawn_cmd` test ve iç kullanım içindir ve Tauri
komutu olarak dışa açılmaz.
- [ ] **Adım 2–4:** Testleri FAIL gör, uygula, PASS gör.
  - `portable-pty` kullanılır: `native_pty_system()`.
  - Her PTY için bir okuma iş parçacığı çalışır ve `Channel` üzerinden
    gönderir.
  - Süreç bitince `on_exit` tetiklenir ve PTY listeden düşer.
- [ ] **Adım 5:** Commit: `feat(app): conpty terminal manager`

### Görev 11: Arayüz iskeleti, Dashboard ve status bar

**Dosyalar:** Oluştur:
- `ui/src/api.ts`, `ui/src/format.ts`
- `ui/src/App.tsx`
- `ui/src/components/{Sidebar,StatusBar,SessionCard,KpiRow}.tsx`
- `ui/src/pages/Dashboard.tsx`
- testler

**Arayüzler:**
- Tüketir: `get_snapshot`, `get_history`, `"snapshot"` olayı (Görev 9)
- Üretir:
  - `api.ts`: Rust tiplerinin TS karşılıkları (camelCase), `onSnapshot(cb): Unlisten`
    ve her komut için bir sarmalayıcı.
  - `App` içinde seçim durumu: `view: 'dashboard' | 'history' | {session: string} | {terminal: number}`.

- [ ] **Adım 1: Başarısız testleri yaz** (Vitest + Testing Library)

```ts
test('kart durum rozetleri', ...)        // Busy→"çalışıyor", Waiting→"onay bekliyor", Idle→"boşta", Other("x")→"x"
test('ajan metni', ...)                  // total 3 finished 1 → "3 ajan · 1 bitti"; total 0 → "ajan yok"
test('bilinmeyen değerler ? gösterir')   // contextPct null → "?" ; model null → "?"
test('KPI bugünün maliyeti tamamlanmadıysa "hesaplanıyor"')
test('status bar seçili oturum yokken "N açık oturum"')
test('format: 1234→"1.2k", 1_500_000→"1.50M", 3.4216→"$3.42"')
```
- [ ] **Adım 2–4:** Testleri FAIL gör, uygula, PASS gör.
  - Düzen ve renkler `.superpowers/brainstorm/…/layout.html` dosyasındaki B
    taslağına göre.
  - Kenar renkleri: yeşil busy, sarı waiting, gri idle.
  - Kart tıklanınca `view={session}` olur.
- [ ] **Adım 5: Elle doğrula.** `npm run tauri dev` çalıştır. Açık oturumlar 2
  saniye içinde kart olarak görünür. Başka bir terminalde bir oturuma iş
  verildiğinde, o kartın rozeti "çalışıyor"a döner.
- [ ] **Adım 6:** Commit: `feat(ui): dashboard, sidebar, status bar`

### Görev 12: Oturum ayrıntısı ve Geçmiş sayfası

**Dosyalar:** Oluştur: `ui/src/pages/{SessionDetail,History}.tsx` ve testleri

**Arayüzler:**
- Tüketir:
  - `get_session_detail`, `get_history`, `open_folder`
  - Görev 13'ten `terminalForPid(parentPid): number | undefined`.
    Görev 13'ten önce bu fonksiyon `undefined` döndüren bir taslaktır.

- [ ] **Adım 1: Başarısız testleri yaz**

```ts
test('ağaç: workflow → aşama → ajan iç içe çizilir')
test('ağaçta ajan seçilince sağdaki liste o ajanın tool çağrılarını gösterir')
test('tool satırı: Ok "✓", Error "✗", Running "… çalışıyor"')
test('"Terminale git" yalnızca terminalForPid tanımlıyken görünür')
test('Geçmiş: summary null satırda model/maliyet/ajan "?"')
test('Geçmiş: klasör filtresi satırları süzer')
```
- [ ] **Adım 2–4:** Testleri FAIL gör, uygula, PASS gör.
  - Düzen `detail.html` taslağına göre.
  - Ayrıntı sayfası açıkken her `"snapshot"` olayında
    `get_session_detail` yeniden çağrılır.
  - Geçmiş sayfası `indexing` true olduğu sürece 2 saniyede bir yenilenir.
- [ ] **Adım 5:** Commit: `feat(ui): session detail and history`

### Görev 13: Terminal görünümü

**Dosyalar:** Oluştur:
- `ui/src/terminal/theme.ts`
- `ui/src/pages/TerminalView.tsx`
- `ui/src/terminal/store.ts` (açık terminaller ve `terminalForPid`)
- testler

**Arayüzler:**
- Tüketir: `pty_*` komutları (Görev 10), `Snapshot` (Görev 9)
- Üretir:
  - `ubuntuTheme: ITheme`: spec 6.5'teki 20 değer birebir.
  - `fontFamily: 'Consolas'`, `fontSize: 13`. Spec'teki 10 punto Windows
    Terminal ölçüsüdür; CSS'te 13px'e karşılık gelir.
  - `terminalForPid`

- [ ] **Adım 1: Başarısız testleri yaz**

```ts
test('tema spec değerleriyle birebir')   // background '#300A24', brightCyan '#34E2E2' … 20 alan
test('terminal adı: içindeki oturumun adı, yoksa "PowerShell N"')
test('terminalForPid: oturumun parentPid değeri bir PTY pid ise o terminal id')
test('kapatma: içinde Busy/Waiting oturum varsa onay ister')
test('süreç çıkınca terminal menüden düşer')
```
- [ ] **Adım 2–4:** Testleri FAIL gör, uygula, PASS gör.
  - `@xterm/xterm` + `@xterm/addon-fit` kullanılır.
  - Terminal sekmesi değişince xterm örneği yok edilmez, gizlenir; böylece
    çıktı ve geri kaydırma geçmişi korunur.
  - Pencere kapatılırken `onCloseRequested` ile aynı onay sorulur.
- [ ] **Adım 5: Elle doğrula.**
  - "＋ Yeni terminal"e bas. Renkler Windows Terminal'deki Ubuntu temasıyla
    aynı olmalı.
  - `xd -help` çalışmalı.
  - `xd <kısayol>` ve ardından `claude` çalıştır. Claude Code'un arayüzü ve iki
    satırlık status bar'ı bozulmadan görünmeli.
  - Sol menüdeki nokta oturumun durumunu izlemeli. Kartın ayrıntısında
    "Terminale git" görünmeli.
  - Kopyala/yapıştır ve pencere boyutlandırma çalışmalı.
- [ ] **Adım 6:** Commit: `feat(ui): embedded powershell terminals with ubuntu theme`

### Görev 14: Paketleme ve kabul

**Dosyalar:** Değiştir: `src-tauri/tauri.conf.json` (bundle: nsis, icon), `README.md` (oluştur: kurulum, geliştirme)

- [ ] **Adım 1:** `npm run tauri build` çalıştır. Beklenen: `src-tauri/target/release/bundle/nsis/Claude Pano_*_x64-setup.exe`.
- [ ] **Adım 2: Spec 9'daki kabul ölçütlerini elle doğrula.** Her madde için
  sonucu commit mesajına yaz. Ağ kontrolü:
  - Uygulama açıkken `Get-NetTCPConnection -OwningProcess <pid>` ve
    `Get-NetUDPEndpoint -OwningProcess <pid>` boş olmalı. Geliştirme modunda
    Vite'ın 1420 portu sayılmaz; kontrol release derlemesinde yapılır.
  - `settings.json` dosyasının hash'i, uygulama açılmadan önce ve kapatıldıktan
    sonra aynı olmalı.
- [ ] **Adım 3: Güvenlik CI'ı ekle.** `.github/workflows/ci.yml` dosyası her
  push ve PR'da `windows-latest` üzerinde şunları çalıştırır:
  - `cargo test --workspace` ve `npm --prefix ui test`
  - `cargo audit` ve `npm audit --audit-level=high` (iki dizinde)
  - `gitleaks` ile tüm geçmişte sır taraması

  Tüm action'lar commit SHA'sına sabitlenir. Workflow'un yetkisi
  `permissions: contents: read` ile sınırlıdır. Beklenen: ilk koşu yeşil.
- [ ] **Adım 4: Public öncesi hazırlık.** Şu dosyalar eklenir:
  - `SECURITY.md`: açık bildirme yolu olarak GitHub private vulnerability
    reporting.
  - `LICENSE`: kullanıcının diğer projeleri gibi MIT. Kullanıcıya teyit
    ettirilir.

  `git log -p` çıktısında kişisel yol, e-posta ya da oturum kimliği
  olmadığı doğrulanır.
- [ ] **Adım 5:** Commit: `chore: release bundle, security ci and acceptance notes`
