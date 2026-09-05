# ARZ Predictor lisans sunucusu

Bu paket masaüstü SQLite veritabanından ayrıdır. Supabase Edge Functions lisans API’sini, Postgres ise lisans ve audit kayıtlarını barındırır.

## Kurulum

1. Supabase projesi oluşturun ve `supabase/migrations/001_license_server.sql` migration’ını uygulayın (`supabase db push` veya SQL Editor).
2. Sunucu ortamında `SUPABASE_URL`, `SUPABASE_SERVICE_ROLE_KEY`, `ARZ_LICENSE_SIGNING_KEY_B64` ve `ARZ_LICENSE_VERIFYING_KEY_B64` secret’larını tanımlayın.
3. Ed25519 anahtar çifti üretin. Private key PKCS#8, public key SPKI DER Base64 olmalıdır. Private key yalnız Supabase secret olarak kalır; repository’ye veya Predictor’a koyulmaz.
4. Fonksiyonları deploy edin: `supabase functions deploy license-activate`, `license-verify`, `admin-license-create`, `admin-license-get`, `admin-license-list`, `admin-license-update`, `admin-license-reset-device`, `admin-license-audit`.
5. Admin fonksiyonları, Supabase Auth kullanıcısının `app_metadata.role = "admin"` olmasını ister. Service-role key hiçbir zaman desktop uygulamaya verilmez.
6. Project URL’yi masaüstü provider yapılandırmasına sonraki entegrasyon adımında ekleyin.
7. Önce TEST_ONLY Ed25519 anahtarıyla test lisansı üretip tek aktivasyon deneyin. Production private key’i test anahtarıyla değiştirmeyin.

## API özeti

Client fonksiyonları `license_key`, `device_fingerprint_hash`, `app_version` ve `token_version: 1` alır. Başarılı cevapta `activation_token`, canonical lisans özeti, server zamanı, `online_revalidate_after` ve `offline_grace_until` döner. Raw key yalnız `admin-license-create` cevabında bir kez döner; list/get uçları yalnız maskesiz olmayan özet döndürür.

`admin-license-update` action değerleri `SUSPEND`, `REACTIVATE`, `REVOKE`, `EXTEND`; `admin-license-reset-device` binding version’ı artırır ve lisansı `UNUSED` yapar. `claim_license_activation` RPC’si satır kilidiyle eşzamanlı aktivasyonu atomik hale getirir.

`admin-license-audit`, bir lisansın denetim kayıtlarını en yeniden eskiye döndürür. Yanıt açık bir alan izin listesi kullanır; key hash, cihaz fingerprint hash’i ve secret değerleri dışarı açılmaz.

## Güvenlik

Key hash’i, cihaz fingerprint hash’i ve audit metadata dışında ham anahtar veya ham donanım kimliği saklanmaz. Loglara token/private key/raw key yazılmaz. `ARZ_LICENSE_SIGNING_KEY_B64` yalnız server-side secret’tır; desktop’a yalnız eşleşen public verifying key dağıtılabilir.

Offline grace politikası merkezi olarak `functions/_shared/license.ts` içindedir: aylık 3, yıllık 7, ömür boyu 14 gün. Token yeniden doğrulama zamanı ve grace sonu server zamanı ile üretilir.

Yerel Deno kurulumu varsa testleri `deno test supabase/tests/license.test.ts` ile çalıştırın. Bu makinede Supabase CLI/Deno yoksa migration ve Edge Function testleri deploy ortamında çalıştırılmalıdır.
