# SessionStation

**Sessions, agents and terminals for Claude Code.** A Windows desktop app
(Tauri 2 + Rust core + React/TypeScript UI) that shows your local Claude Code
sessions, their subagents and costs, and runs terminals next to them.

> Unofficial; not affiliated with or endorsed by Anthropic. Claude and Claude
> Code are trademarks of Anthropic.

Claude Code oturumlarını, alt ajanlarını ve terminallerini tek pencerede
izleyen Windows masaüstü uygulaması.

> Ekran görüntüleri: yakında eklenecek.

## Özellikler

- Çalışan Claude Code oturumlarının canlı panosu: durum, bağlam kullanımı, token ve maliyet
- Oturum ayrıntısı: son tool çağrıları, alt ajan ağacı
- Geçmiş: günlük maliyet ve oturum dizini; kapanmış oturumu kendi proje klasöründe sürdürme
- Gömülü terminal, "Windows Terminal'de aç" ve "Klasörü aç"
- Arayüz sekiz dilde: Türkçe, Azərbaycan, English, Deutsch, Русский, Español, Français, 简体中文

## Gizlilik ve güvenlik

- `~/.claude` (veya `CLAUDE_CONFIG_DIR`) altında yalnızca okur; hiçbir şey yazmaz
- Dış ağ isteği yok, telemetri yok, ağa açık port yok
- Hook kurmaz, Claude Code ayarlarını değiştirmez
- Arayüz ile çekirdek yalnızca Tauri IPC ile konuşur; her komut bir izinle
  sınırlandırılmıştır ve CSP dış kaynağa izin vermez

Güvenlik açığı bildirimi için `SECURITY.md` dosyasına bakın.

## Kurulum

Release sayfasından NSIS kurulum dosyasını (`SessionStation_*_x64-setup.exe`)
indirip çalıştırın. Kurulum kullanıcı bazlıdır, yönetici yetkisi gerekmez.
Kurulum dosyası henüz imzalı değildir; Windows SmartScreen uyarı gösterebilir.
`LICENSE` ve `THIRD_PARTY_NOTICES.md` uygulamanın yanına kurulur.

## Geliştirme

Gereksinimler: Rust (MSVC araç zinciri) + Visual Studio Build Tools, Node 22.

```powershell
npm ci
npm --prefix ui ci
npm run tauri dev      # geliştirme
npm run tauri build    # NSIS paketi: target/release/bundle/nsis/
cargo test --workspace
npm --prefix ui test
node scripts/gen-notices.mjs   # THIRD_PARTY_NOTICES.md'yi yerel meta veriden yeniden üretir
```

## Uyarı

SessionStation, Claude Code'un belgelenmemiş iç dosyalarını (oturum kayıtları,
`sessions/*.json`, alt ajan meta dosyaları) okur. Bu biçimler Claude Code
güncellemeleriyle değişebilir; bilinmeyen alanlar atlanır, ancak bazı bilgiler
eksik ya da hatalı görünebilir.

SessionStation resmî bir Anthropic ürünü değildir; Anthropic ile bağlantılı
değildir ve Anthropic tarafından onaylanmamıştır. Claude ve Claude Code,
Anthropic'in ticari markalarıdır.

## Lisans

MIT, bkz. `LICENSE`. Üçüncü taraf lisansları: `THIRD_PARTY_NOTICES.md`.
