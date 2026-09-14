# Günlük otomatik akış ve yalnızca bugünün kuponları

Üretim AppData ve gerçek Tauri masaüstü üzerinde doğrulandı. İş günü: **2026-09-13**, Europe/Istanbul. Son iki tam yenileme: denetim #6 ve #7. Uygulama son doğrulama sonunda gerçek saate ve bugünün Kuponlar ekranına geri bırakıldı.

## Otomatik üretim

İddaa yenileme komutu artık içe aktarma → artımlı kimlik çözümleme → BASE tahminlerinin güvenli yeniden kullanımı/eksik tahminlerin üretimi → aday/kupon yayınları → Popülerler güncellemesini kendisi yürütüyor. İstemcide ikinci bir manuel hazırlama çağrısına bağlı değil.

- Provider competition/event eşlemeleri önce okunur. Bilinen olayın kanonik ev/deplasman kimliği yeni yazımlarda korunur.
- Gözlenen bülten takım adı (hn/an) veriyor; ayrı, doğrulanmış sağlayıcı takım ID alanı yok. Sayısal takım ID'si uydurulmadı. Takım kimliği mevcut eşlemeler, sağlayıcı + kanonik lig kapsamlı takma adlar ve olayın ev/deplasman kimliğiyle saklanıyor.
- Yeni kimliklerde sıra: kayıtlı kimlik → kapsamlı takma ad → kesin/kurallı isim → lig içi güvenli benzerlik → manuel inceleme. Aynı ülkedeki terfi/düşme için tekil kesin kimlik kullanılabilir; lig dışına bulanık eşleştirme genişletilmez. Mevcut 0,92 güven / 0,05 kazanan farkı değiştirilmedi.
- Yeni kalıcı kuyruk: NEW, AUTO_RESOLVING, RESOLVED, RETRY_LATER, AMBIGUOUS, UNSUPPORTED, MANUAL_REVIEW. Açılış ve 60 saniyelik kontrol yalnızca güncel/gelecek işleri işler; parti sınırı 64/128. Kesilmiş işler geri alınır, geçici hatalar yeniden denenir, belirsizlikler günlük yeniden kontrolden önce bekletilir.
- Teknik ayrıntı güncel inceleme satırını, olay ID'sini, ligi, takımları, başlama saatini, olası kanonik takımları ve güven puanlarını gösterir.
- 18 kapsamlı takım takma adı; 2696 olay/pazar/seçim/çizgi kapsamlı sağlayıcı seçim eşlemesi kalıcı olarak saklandı.

Başlangıçta güncel/gelecek evrenden oluşturulan 9 kontrol işi 9 RESOLVED durumuna geçti. Son iki yenilemede yeni desteklenen bilinmeyen takım yoktu: 9 mevcut BASE tahmini yeniden kullanıldı, yeniden çözümleme gerektiren iş 0, bekleyen iş 0. Yeni/belirsiz kimlik davranışı dört gün regresyonunda ayrıca sınandı; canlı döngülerde ortaya çıkmamış yeni takım senaryosu olmuş gibi raporlanmıyor.

Küresel çözümleme denetimi **404 → 404**: global rebuild çalışmadı. SQLite planı aktif maçlar için idx_matches_status_kickoff indeksini, kuyruk için birincil anahtar aramasını kullanıyor. Son iki bültenin her biri 163 olayı güncelledi, mükerrer olay eklemedi; lig ve Popülerler aktarımlarında da satır hatası 0.

## Gerçek ölçümler

Tüm süreler ms. Toplam, gerçek düğme tıklamasından işlem sonuna kadardır; ağ, Popülerler ve ekran yenilemesi dahildir. İçe aktarma süresi bültenin ağ edinimini de içerir.

| Denetim / bülten import | Toplam | Bülten edinme + içe aktarma | Artımlı çözümleme | Tahmin/yayın | Durum sayımları | Popülerler hatası |
|---|---:|---:|---:|---:|---:|---:|
| 6 / 82 | 21007 | 5901 | 1 | 3138 | 37 | 0 |
| 7 / 85 | 11173 | 2158 | 0 | 3758 | 34 | 0 |

50 ms arayüz zamanlayıcısının en uzun gözlenen aralığı 65.6 ms. Ekran gezinmesi yanıt verdi; üretim işlemleri arayüzü uzun süre durdurmadı.

İlk kabul denemesi BASE sorgusunda yanlış bir revizyon alanı yakaladı. Bu hata düzeltildi ve şema regresyonu eklendi. Başarısız günlük üretim kayıtları artık hazırlıkta görünür, zamanlanmış yeniden denemeye girer. Ayrı bir Popülerler denemesinde 20/20 satır hatası önceki kod tarafından tamamlandı sayılmıştı; eski kayıt ayrıntılı satır nedenlerini saklamıyordu. Ertelenmiş SQLite okuma/yazma geçişi kaldırıldı, yazıcı baştan rezerve edildi, her seçim savepoint ile atomik hale getirildi ve tüm satırların başarısızlığı FAILED oldu. Yukarıdaki iki kabul döngüsü bu düzeltmelerden sonra hatasız tamamlandı.

## Yayın, tarih ve geçmiş

Bugün / Adaylar / Kuponlar / Popülerler gerçek ekranda aynı yayın kimliğini gösterdi: **2026-09-13:base:63:btts:80**. BASE durumu doğru; olmayan 11 modeli iddia edilmiyor.

Test saatinde bugünkü desteklenen olaylar canlı bültenden çıkmıştı: bugünkü aktif desteklenen/eşleşen/eşleşmeyen **0 / 0 / 0**, gelecek desteklenen/eşleşen/eşleşmeyen **9 / 9 / 0**, gelecek model hazır **9**. Yeni boş gün içi üretim, bugünün dolu ve donmuş yayınını gizlemedi. 14 Eylül için 43 kategori seçim satırı ve 4 kullanılabilir kupon otomatik üretildi; son gelecek gün yayını **2026-09-14:base:93:btts:93**.

| Günlük aday sekmesi | Önce | Sonra |
|---|---:|---:|
| Tümü | 64 | 64 |
| Korner | 16 | 16 |
| 2.5 Üst | 3 | 3 |
| 3.5 Üst | 3 | 3 |
| KG Var | 17 | 17 |
| Yüksek Güven | 11 | 11 |
| Sürpriz | 8 | 8 |
| Katlama | 17 | 17 |

- Kuponlar, Adaylar ve Popülerler tarih seçicileri kaldırıldı. Bugün/Adaylar/Kuponlar ortak İstanbul saatine bağlı; Yenile günlük ekranın otomatik tarihini kullanır.
- Gece yarısı gerçek WebView saat bağımlılığı simüle edilerek **2026-09-13 → 2026-09-14 → 2026-09-13** doğrulandı. Üç günlük ekran yeni yayınla birlikte değişti; eski asenkron yanıtlar eski günü yeni başlık altında göstermedi.
- Açılışta gerçek tarih kullanıldı. Kuponlar ilk açıldığında bugünün yayını yüklendi; kayıtlı bir seçili tarih yok.
- Eski günün bekleyen katlama kuponu günlük kartlara eklenmiyor. Seri bekleme durumu korunuyor; önceki adımların günlük ekranda tarihli kupon listesi kaldırıldı.
- **64 → 64** tarihsel kupon: üretim öncesi SQLite yedeğiyle tüm kayıt alanları birebir aynı. Settlement/ROI/geçmiş verisi silinmedi. Model ve Kupon Performansı ekranları günlük yayın değişiminde yeniden bağlanıp filtre kaybetmiyor.

## Son hazırlık

| Temel kontrol | Sonuç |
|---|---|
| Yerel veritabanı | READY |
| Canlı İddaa ve oranlar | READY |
| Üretim geçmiş verisi | READY |
| BASE tahmin modeli | READY |
| Kalibrasyon | READY |
| Özellik verisi | READY |
| Güncel/gelecek eşleştirme | READY |
| Aday üretimi | READY |
| Kupon üretimi | READY |
| Popülerlik verisi | READY |

Genel durum **READY (10/10)**. Tahmin/aday/kupon üretilebilir: **EVET / EVET / EVET**. Lineup: OPTIONAL; logolar: BACKGROUND. İsteğe bağlı modüller paydaya eklenmedi.

## Doğrulama ve dosyalar

15 frontend testi geçti. Artımlı çözümleme (dört gün + kesilmiş iş), BASE şema kontrolü, 19 migrasyon/veritabanı testi, İddaa/Popülerler aktarım testleri, hazırlık, yayın, BTTS ve Popülerler projeksiyon regresyonları geçti. Dört günlük senaryo terfi eden takımın kesin kimliğini ve ilgisiz ligde aynı isim bulunurken belirsizliğin izolasyonunu da kapsıyor. TSC/Vite ve gerçek masaüstü derlemesi başarılı.

Başlıca kaynaklar: [artımlı çözümleme](src-tauri/src/repositories/incremental_resolution.rs), [günlük işçi](src-tauri/src/daily_pipeline.rs), [migrasyon 27](src-tauri/migrations/0027_incremental_resolution.sql), [ortak saat](src/lib/businessClock.ts), [Kuponlar](src/pages/CouponsPage.tsx).

Kanıtlar: [son DB denetimi](.tmp-dataflow/daily-self-healing/final-audit.json), [iki yenileme](.tmp-dataflow/daily-self-healing/cycles.json), [hazırlık](.tmp-dataflow/daily-self-healing/readiness.json), [ekran yayınları](.tmp-dataflow/daily-self-healing/pages.json), [gece yarısı](.tmp-dataflow/daily-self-healing/rollover.json), [Kuponlar ekranı](.tmp-dataflow/daily-self-healing/coupons.png), [gece yarısı ekranı](.tmp-dataflow/daily-self-healing/coupons-rollover.png), [Veri Merkezi](.tmp-dataflow/daily-self-healing/data-center.png).

Üretim yedeği: .tmp-dataflow/production-2026-09-13T20-14-58-285Z.sqlite3. Lisans/updater ve tahmin modeli tasarımı değiştirilmedi.
