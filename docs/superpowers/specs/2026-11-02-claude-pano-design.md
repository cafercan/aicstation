# claude-pano — Tasarım

Tarih: 2026-11-02
Durum: Taslak, onay bekliyor

## 1. Amaç ve neden

Kullanıcı aynı anda farklı klasörlerde birkaç Claude Code oturumu açık tutuyor ve
bunları Windows Terminal sekmeleri arasında gezerek takip ediyor. Hangi oturumun
çalıştığını, hangisinin bittiğini, hangisinin onay beklediğini ve kaç alt ajan
çalıştırdığını görmek için tek tek sekmelere bakmak zorunda.

claude-pano, makinedeki tüm açık Claude Code oturumlarını tek ekranda gösteren ve
aynı pencerede gerçek PowerShell terminalleri açabilen bir masaüstü uygulamasıdır.

Mevcut araçlar denendi ve elendi:

- claude-visual: yalnızca hook'larla veri topluyor, zaten açık olan oturumları
  göremedi. Sunucusu şifresiz ve tüm ağ arayüzlerinde dinliyordu.
- claude-view: yalnızca macOS'ta çalışıyor.
- CC GUI (desktop-cc-gui): her açılışta uzak bir analiz script'ini (Baidu)
  dosya ve kabuk erişimi olan pencereye yüklüyor, kapatma ayarı yok. Ayrıca
  başka terminallerdeki oturumları canlı izlemiyor.

Bu deneyimlerden çıkan kesin kurallar Bölüm 3'te.

## 2. Hedefler

1. **Dashboard:** Yalnızca şu an açık olan Claude Code oturumları kartlar halinde
   görünür. Uygulama içinden ya da dışarıdan (Windows Terminal, VS Code vb.)
   açılmış olmaları fark etmez.
2. **Oturum ayrıntısı:** Karta tıklayınca o oturumun ayrıntı sayfası açılır.
3. **Geçmiş sekmesi:** Kapanmış oturumlar burada listelenir ve aynı ayrıntı
   sayfasıyla açılır.
4. **Gömülü terminal:** Uygulama içinde gerçek PowerShell oturumları açılır.
   Kullanıcının profili, `xd` modülü ve Windows Terminal'deki "Ubuntu" renk teması
   aynen gelir. Kullanıcı `xd` ile klasöre gidip `claude` başlatır.
5. **Status bar:** Uygulamanın altında seçili oturumun bilgisini gösteren bir çubuk
   bulunur. Terminallerin içinde de Claude Code'un kendi status bar'ı görünür.

## 3. Hedef dışı ve kesin kurallar

Hedef dışı (ilk sürümde yok):

- Uygulamadan Claude'a mesaj yazmak, onay vermek ya da oturumu durdurmak.
  Uygulama sadece izler; etkileşim terminalde yapılır.
- MCP sunucusu (bkz. Bölüm 8).
- Linux desteği. Bu ikinci aşama, ama mimari engel çıkarmamalı (bkz. Bölüm 7).
- Dışarıdan açılmış bir terminal penceresine geçmek ya da onu uygulamaya almak.
- 5 saatlik ve haftalık kullanım limitlerini uygulamanın status bar'ında göstermek.
  Bu bilgi kayıt dosyalarında yok.

Kesin kurallar (anti-goals). Bunlardan biri çiğnenirse uygulama çalışsa bile
başarısız sayılır:

- **Hook yok.** `~/.claude/settings.json` ve Claude Code yapılandırmasına hiçbir
  şey yazılmaz. Uygulama Claude Code dosyalarını yalnızca okur.
- **Telemetri yok, dış ağ isteği yok.** Uygulama hiçbir uzak adrese bağlanmaz,
  otomatik güncelleme kontrolü de yapmaz. Arayüze uzak script, font ya da CDN
  yüklenmez; her şey pakete gömülüdür.
- **Ağa açık port yok.** Arayüz ile çekirdek arasındaki iletişim Tauri IPC
  üzerinden olur. Bir HTTP sunucusu gerekirse yalnızca `127.0.0.1`'e bağlanır.
- **Gerçek terminal davranışı.** Kopyala/yapıştır, odak, yeniden boyutlandırma,
  renkler ve Claude Code'un tam ekran arayüzü sorunsuz çalışmalıdır. Bu yüzden ayrı
  bir konsol penceresini yakalayıp içine yapıştırma (SetParent) yöntemi
  kullanılmaz.

## 4. Kısıtlar ve ortam

- İşletim sistemi Windows 11. PowerShell 5.1 (`powershell.exe`) varsayılan kabuk.
- Kullanıcının mevcut Tauri + Rust projeleri var (ör. `c-launcher`). Rust araç
  zinciri ve Node 22 kurulu.
- Proje klasörü: `D:\Workspaces\AI\claude-pano`.
- Arayüz dili Türkçe.
- Claude Code sürümü yazım anında 2.1.31x. Aşağıdaki dosya biçimleri Claude
  Code'un **iç ve belgelenmemiş** biçimleridir; her sürümde değişebilir.

## 5. Veri kaynakları (incelenmiş gerçek yapılar)

### 5.1 Canlı oturum kaydı: `~/.claude/sessions/<pid>.json`

Çalışan her Claude Code süreci için bir dosya bulunur. Gözlenen alanlar:

```json
{
  "pid": 12345,
  "sessionId": "00000000-…",
  "cwd": "D:\\Projects\\ornek",
  "startedAt": 1793351812840,
  "procStart": "134378254092118102",
  "version": "2.1.314",
  "kind": "interactive",
  "entrypoint": "cli",
  "name": "ornek",
  "status": "idle",
  "updatedAt": 1793358961715,
  "statusUpdatedAt": 1793358961715
}
```

`status` alanında görülen değerler `idle`, `busy` ve `waiting`. Bu üçü kartta
sırasıyla "boşta", "çalışıyor" ve "onay bekliyor" olarak gösterilir. `waiting`
durumunda bazen bir `waitingFor` alanı da bulunur (ör. "input needed"); varsa
kartta gösterilir. Bilinmeyen bir `status` değeri gelirse olduğu gibi
gösterilir. Açık oturum
listesinin birincil kaynağı bu dosyalardır.

Aynı klasörde `*.key` dosyaları da var. Bunlar **okunmaz**: içlerinde gizli
anahtarlar olabilir ve uygulamanın bunlara ihtiyacı yok.

Bir oturumun açık sayılması için iki koşul aranır: `pid` ile çalışan bir süreç
olmalı ve sürecin başlangıç zamanı `procStart` ile eşleşmeli. İkinci koşul, aynı
PID'in başka bir süreç tarafından yeniden kullanılmasına karşı korur. Koşullar
sağlanmazsa oturum kapanmış sayılır ve Geçmiş'e düşer.

`procStart`, sürecin Windows FILETIME cinsinden başlangıç zamanıdır ve
`GetProcessTimes` ile birebir eşleşir. WMI `CreationDate` daha düşük çözünürlüklü
olduğu için bu karşılaştırmada kullanılmaz.

### 5.2 Oturum kaydı: `~/.claude/projects/<kodlanmış-klasör>/<sessionId>.jsonl`

Her satır bir JSON nesnesidir. Kullanılacak satır türleri:

- `assistant`: `message.model`, `message.usage` (token sayıları),
  `message.content[]` içindeki `tool_use` blokları (araç adı, girdi, `id`).
- `user`: `message.content[]` içindeki `tool_result` blokları (`tool_use_id`,
  `is_error`).
- `custom-title` / `agent-name`: oturum adı.
- Ortak alanlar: `timestamp`, `cwd`, `gitBranch`, `sessionId`, `isSidechain`.

Bilinmeyen türler sessizce atlanır.

**Boyut:** Dosyalar çok büyük olabilir. Ölçümde en büyüğü 429 MB, toplamı 2.1 GB
çıktı. Okuma akış halinde ve artımlı yapılır: dosya hiçbir zaman tamamen belleğe
alınmaz, son okunan bayt konumu saklanır ve yalnızca eklenen kısım işlenir.

**Token tekrarı:** Tek bir API yanıtı, aynı `message.id` değerine sahip birden
fazla `assistant` satırına bölünür ve her satır aynı `usage` değerini tekrarlar.
Token ve maliyet hesabında satırlar `message.id`'ye göre tekilleştirilir.

**Model:** `message.model` her zaman temel model kimliğini taşır (ör.
`claude-opus-5-5`). 1M bağlamlı oturumlarda `[1m]` eki yalnızca `requestedModel`
alanında bulunur. Bağlam penceresi bu yüzden `requestedModel`'den belirlenir.

**Kaydı bulma:** Klasör adları geri çözülemez: `_` karakteri `-`'ye dönüşüyor,
ASCII dışı karakterler değiştiriliyor ve sürücü harfinin büyük/küçük harfi
değişebiliyor. Kayıt dosyası `sessionId` ile, bütün `projects` alt klasörlerinde
aranır. Oturumun klasörü satırlardaki `cwd` alanından alınır.

### 5.3 Alt ajanlar: `<projects>/<kodlanmış-klasör>/<sessionId>/subagents/`

Her alt ajan için iki dosya var:

- `agent-<id>.jsonl`: ajanın kaydı. Biçimi 5.2 ile aynıdır, `isSidechain: true`
  ve `agentId` alanları vardır.
- `agent-<id>.meta.json`: ajanın özeti. Gözlenen alanlar: `agentType`,
  `description`, `toolUseId`, `spawnDepth`, `parentAgentId` (derinlik 2 ve
  üstünde), `model` ve `finished`.
  - `model` alanı bir model kimliği değil, bir takma addır ("opus", "sonnet",
    "inherit", "fable"); çoğu zaman da hiç yoktur. Gerçek model ajanın kendi
    `.jsonl` dosyasından okunur.
  - `finished` alanı ya `true` ya da hiç yok. `false` değeri hiç görülmedi.
- `subagents/workflows/wf_*/` altında workflow'ların başlattığı ajanlar bulunur.
  Bunlarda `workflowPhase` alanı vardır, `toolUseId` yoktur.

**Ağacın kurulması:**

- Derinlik 1'deki ajanlar ana oturumun altına girer.
- `parentAgentId` alanı olan ajanlar o ajanın altına girer. Bunların
  `toolUseId` değeri ana kayıtta değil, üst ajanın kaydında bulunur.
- Workflow ajanları, workflow kimliğiyle adlandırılmış bir "workflow" düğümünün
  altında, aşamaya (`workflowPhase`) göre gruplanır. Bu ajanlar da ajan
  sayılarına dahildir.

**Ajan bitti mi?**

- `finished: true` ise bitti.
- Alan yoksa ajan şu üç koşulun **hepsi** sağlanırsa çalışıyor sayılır: oturumu
  açık, kendi `.jsonl` dosyası son 2 dakikada değişmiş ya da son tool çağrısının
  sonucu henüz gelmemiş. Koşullar sağlanmıyorsa bitti sayılır.
- Kapanmış oturumlarda (Geçmiş) tüm ajanlar bitti sayılır.

### 5.4 Durum hesapları

- **Bağlam doluluğu:** Son `assistant` satırındaki `usage` alanında input, cache
  read ve cache creation değerlerinin toplamı, modelin bağlam penceresine bölünür.
  Pencere boyutu model kimliğinden belirlenir; bilinmeyen modelde alan boş
  bırakılır.
- **Maliyet:** Tekilleştirilmiş token sayıları, uygulamaya gömülü bir fiyat
  tablosuyla çarpılır. Tablo kod içinde tek bir yerde durur ve kolayca
  güncellenebilir. Değer tahmini olarak gösterilir. Bir oturumun maliyetine alt
  ajanlarının maliyeti de dahildir.
- **Bugünün toplamı:** Bugün (yerel saatle 00:00'dan beri) zaman damgası taşıyan
  tüm API yanıtlarının toplamıdır. Bugün kapanmış oturumlar ve alt ajanlar dahil.
  Gece yarısını aşan oturumlar zaman damgasına göre bölünür. Hesap, Geçmiş
  dizinleyicisinden (6.4) gelir; dizinleme bitene kadar değerin yanında
  "hesaplanıyor" ibaresi görünür.
- **Tool çağrısının durumu:** Eşleşen bir `tool_result` varsa tamamlandı ya da
  hatalı (`is_error`), yoksa sürüyor.

## 6. Ekranlar

Görsel taslaklar `.superpowers/brainstorm/10057-1793605238/content/` altında
duruyor: `layout.html` (B düzeni seçildi), `terminal.html` ve `detail.html`.

### 6.1 Ana düzen

- **Solda menü:** Dashboard, Geçmiş, "TERMİNALLER" başlığı altında uygulama içi
  terminallerin listesi ve "＋ Yeni terminal" butonu. Her terminalin yanında,
  içinde çalışan Claude oturumunun durum noktası vardır: yeşil çalışıyor, sarı onay
  bekliyor, gri boşta ya da Claude çalışmıyor.
- **Sağda içerik.**
- **En altta uygulamanın status bar'ı:** Seçili oturumun model, klasör adı,
  branch, bağlam yüzdesi, ajan sayısı, durum ve maliyet bilgisi. Seçili oturum
  yoksa bu çubuk açık oturum sayısını gösterir.

### 6.2 Dashboard

- **Üst sıra özet kutuları:** açık oturum sayısı, çalışan alt ajan sayısı, onay
  bekleyen oturum sayısı ve bugünün toplam tahmini maliyeti.
- **Oturum kartları (ızgara).** Her kartta:
  - oturum adı ve klasör yolu,
  - durum rozeti ve kartın sol kenarında durum rengi,
  - "N ajan · M bitti",
  - model,
  - son tool çağrısı,
  - geçen süre,
  - bağlam doluluk çubuğu.
- **Sıralama:** Önce onay bekleyenler, sonra çalışanlar, sonra boştakiler. Her
  grubun içinde son hareket zamanına göre.
- **Güncelleme:** Kartlar 1–2 saniye içinde güncellenir. Dosya değişikliklerini
  izleyen bir mekanizma kullanılır, gerekirse periyodik tarama ile desteklenir.

### 6.3 Oturum ayrıntısı

- **Başlık:** "←" (geri), oturum adı, durum, klasör yolu, başlama saati ve son
  hareket zamanı.
- **Butonlar:** "Terminale git" yalnızca oturum uygulama içindeki bir terminalde
  çalışıyorsa görünür. "Klasörü aç" klasörü Gezgin'de açar.
- **Özet kutuları:** model, bağlam %, toplam token, tahmini maliyet, ajan / biten
  ve tool çağrısı sayısı.
- **Sol, ajan ağacı:** Ana oturum ve alt ajanlar. Her satırda tür, görev
  açıklaması, durum ve süre.
- **Sağ, son tool çağrıları:** Ağaçta seçili ajanın son çağrıları (yaklaşık 50).
  Her satırda saat, araç adı, kısa hedef (dosya adı, komut ya da desen) ve sonuç:
  ✓, ✗ ya da sürüyor. Prompt metinleri ve tool çıktılarının tamamı gösterilmez,
  sadece kısa özetler.

### 6.4 Geçmiş

Kapanmış oturumların listesi: tarih, ad, klasör, model, süre, ajan sayısı ve
maliyet. Liste klasöre göre filtrelenebilir. Bir satıra tıklayınca 6.3'teki
ayrıntı sayfası salt okunur olarak açılır.

Liste `~/.claude/projects` altındaki tüm kayıtlardan oluşur. Toplam veri GB
düzeyinde olduğu için bu iş bir **arka plan dizinleyicisi** tarafından yapılır.
Dizinleyici ilk sürümün temel parçasıdır, sonraya bırakılmaz.

- **İlk açılış:** Liste hemen dosya bilgileriyle görünür: tarih, klasör ve ad.
  Model, süre, ajan sayısı ve maliyet sütunları dizinleme ilerledikçe dolar; o
  ana kadar "?" gösterilir. En yeni oturumlar önce işlenir.
- **Önbellek:** Sonuçlar uygulamanın kendi veri klasöründe saklanır. Sonraki
  açılışlarda yalnızca değişen ya da yeni dosyalar işlenir.
- **Öncelik:** Dizinleyici, canlı oturumların izlenmesini yavaşlatmayacak şekilde
  düşük öncelikle çalışır.

### 6.5 Terminal

- Sağ tarafta tam ekran bir terminal. Altyapı ConPTY'dir: gerçek `powershell.exe`
  süreci başlatılır ve uygulama ekranını çizer (Windows Terminal'in kullandığı
  yöntem).
- PowerShell kullanıcının profiliyle açılır, böylece `xd` modülü yüklenir.
- Başlangıç klasörü kullanıcının ev dizinidir.
- **Renk teması:** Windows Terminal'deki "Ubuntu" şeması.
  - Arka plan `#300A24`, ön plan `#EEEEEC`, imleç `#FFFFFF`, seçim `#B5D5FF`.
  - 16 renk: black `#2E3436`, red `#CC0000`, green `#2DC94E`, yellow `#C4A000`,
    blue `#4C93FF`, purple `#75507B`, cyan `#06989A`, white `#D3D7CF`,
    brightBlack `#555753`, brightRed `#EF2929`, brightGreen `#6BEE85`,
    brightYellow `#FCE94F`, brightBlue `#8FBEFF`, brightPurple `#AD7FA8`,
    brightCyan `#34E2E2`, brightWhite `#EEEEEC`.
  - Yazı tipi Consolas 10.
- Terminalin adı, içindeki Claude oturumunun adı ya da klasör adıdır.
- **Terminal ile oturumu eşleme:** Uygulama başlattığı PowerShell'in PID'ini bilir.
  `sessions/<pid>.json` kayıtlarındaki Claude süreçlerinden üst süreci bu
  PowerShell olanlar o terminale aittir. Bu eşleme "Terminale git" butonunu ve
  sol menüdeki durum noktalarını besler.
- Terminal kapatılırsa (sekme kapatma ya da `exit`), menüden kaldırılır.
  Uygulama kapanırken açık terminaller kapatılır. İçlerinde Claude çalışıyorsa
  önce kullanıcıya onay sorulur.
- Terminallerin uygulama yeniden açıldığında geri gelmesi kapsam dışıdır.
- Status bar için ek bir iş yoktur. Claude Code, kullanıcının `statusline.py`
  dosyasıyla status bar'ı terminalin içinde zaten çizer.

## 7. Mimari

- **Tauri 2 + Rust çekirdek.** Kullanıcının diğer projeleriyle aynı yapı.
  Önerilen paketler: `portable-pty` (ConPTY; Linux'ta Unix PTY), `notify` (dosya
  izleme) ve `sysinfo` ya da Windows API (süreç varlığı, başlangıç zamanı, üst
  süreç).
- **React + TypeScript arayüz, `xterm.js` terminal.** Tüm bağımlılıklar pakete
  gömülür.
- **Katmanlar:**
  1. **Okuyucu katman (`core`):** Bölüm 5'teki kaynakları okur ve
     `Session` / `Agent` / `ToolCall` modellerini üretir. Arayüzden bağımsız, saf
     Rust kütüphanesidir. Örnek JSONL dosyalarıyla birim testleri yazılır.
     İleride MCP buradan beslenir.
  2. **PTY katmanı:** terminal oluşturma, yazma, yeniden boyutlandırma ve kapatma.
  3. **Tauri komutları ve olayları:** Arayüze anlık görüntü ve değişiklik
     olayları gönderir.
- **Platform ayrımı:** Windows'a özgü parçalar (ConPTY, süreç sorgusu,
  `powershell.exe`) tek bir modülün arkasında durur. Linux'ta bunların yerine bash
  ve `/proc` kullanılır (ikinci aşama). Dosya yolları `~/.claude` üzerinden
  çözülür; `CLAUDE_CONFIG_DIR` ortam değişkeni tanımlıysa ona uyulur.
- **Sağlamlık:** Biçim belgelenmemiş ve değişebilir. Okuyucu bilinmeyen alanları
  ve türleri atlar, bozuk satırda çökmez. Kritik bir alan bulunamazsa kartta o
  bilgi "?" olarak görünür. Uygulama Claude Code'un hiçbir dosyasına yazmaz.

## 8. MCP (sonraki aşama)

İlk sürümde yok. Okuyucu katman ayrı tutulduğu için, istenirse sonra salt okunur
bir MCP sunucusu eklenebilir: oturumları listeleme ve oturum ayrıntısı alma.
Terminal açma gibi yan etkili araçlar ayrıca tartışılır.

## 9. Başarı ölçütü

- Uygulama açıldığında o an çalışan tüm Claude Code oturumları, nereden açılmış
  olurlarsa olsunlar, 2 saniye içinde kart olarak görünür. Durumları
  (çalışıyor / onay bekliyor / boşta) terminaldeki gerçek durumla uyuşur.
- Bir oturum alt ajan başlattığında kartın ajan sayısı ve ayrıntıdaki ağaç birkaç
  saniye içinde güncellenir; ajan bitince "bitti" olarak işaretlenir.
- Bir oturum kapatıldığında kartı Dashboard'dan kalkar ve Geçmiş'te görünür.
- Uygulama içinde açılan terminalde `xd <kısayol>` ve `claude` sorunsuz çalışır.
  Renkler Windows Terminal'deki Ubuntu temasıyla aynıdır. Claude Code'un tam ekran
  arayüzü ve status bar'ı bozulmadan görünür.
- Uygulama çalışırken dış ağa tek bir bağlantı açılmaz, `settings.json`
  değişmez.

## 10. Uygulayıcıya bırakılanlar

- Paket ve kütüphane sürümleri, klasör yapısı, durum yönetimi kütüphanesi.
- Fiyat tablosunun içeriği ve model–bağlam penceresi eşlemesi.
- Dosya izleme ile periyodik taramanın dengesi.
- Geçmiş önbelleğinin biçimi ve yeri (uygulamanın kendi veri klasöründe olmalı).
- Görsel ayrıntılar: kart ızgarasının sütun sayısı, yazı tipleri, karanlık tema
  tonları. Taslaklar yön gösterir, birebir bağlayıcı değildir.
