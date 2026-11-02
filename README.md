# Claude Pano

Claude Code oturumlarını tek pencerede izleyen Windows masaüstü uygulaması
(Tauri 2 + Rust çekirdek + React/TypeScript arayüz).

> Ekran görüntüleri: yakında eklenecek.

## Özellikler

- Çalışan Claude Code oturumlarının canlı panosu: durum, bağlam kullanımı, token ve maliyet
- Oturum ayrıntısı: son tool çağrıları, alt ajan ağacı
- Geçmiş: günlük maliyet ve oturum dizini
- Gömülü terminal ve "Klasörü aç"
- Arayüz Türkçe

## Gizlilik ve güvenlik

- `~/.claude` (veya `CLAUDE_CONFIG_DIR`) altında yalnızca okur; hiçbir şey yazmaz
- Dış ağ isteği yok, telemetri yok, ağa açık port yok
- Hook kurmaz, Claude Code ayarlarını değiştirmez
- Arayüz ile çekirdek yalnızca Tauri IPC ile konuşur; CSP dış kaynağa izin vermez

Güvenlik açığı bildirimi için `SECURITY.md` dosyasına bakın.

## Kurulum

Release sayfasından NSIS kurulum dosyasını (`Claude Pano_*_x64-setup.exe`)
indirip çalıştırın. Kurulum kullanıcı bazlıdır, yönetici yetkisi gerekmez.

## Geliştirme

Gereksinimler: Rust (MSVC araç zinciri) + Visual Studio Build Tools, Node 22.

```powershell
npm ci
npm --prefix ui ci
npm run tauri dev      # geliştirme
npm run tauri build    # NSIS paketi: target/release/bundle/nsis/
cargo test --workspace
npm --prefix ui test
```

## Uyarı

Claude Pano, Claude Code'un belgelenmemiş iç dosyalarını (oturum kayıtları,
`sessions/*.json`, alt ajan meta dosyaları) okur. Bu biçimler Claude Code
güncellemeleriyle değişebilir; bilinmeyen alanlar atlanır, ancak bazı bilgiler
eksik ya da hatalı görünebilir.

## Lisans

MIT, bkz. `LICENSE`.
