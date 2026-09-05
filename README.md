# ARZ Predictor

ARZ Predictor, Tauri 2, React, TypeScript ve Vite tabanlı lisanslı Windows masaüstü uygulamasıdır.

## Geliştirme

```powershell
npm.cmd install
npm.cmd run tauri dev
```

Lisans istemcisi yalnız public değerleri Rust derleme ortamından alır:

- `SUPABASE_URL`
- `SUPABASE_PUBLISHABLE_KEY`
- `ARZ_LICENSE_VERIFYING_KEY_B64`

Service-role/secret anahtarları, admin kimlikleri ve private signing key'ler Predictor'a verilmemelidir. Aktivasyon kaydı `com.footballpredictor.app` AppData dizinindeki `activation.json` dosyasında imzalı token olarak saklanır; ham lisans anahtarı saklanmaz.

## Doğrulama ve üretim build'i

```powershell
npm.cmd test
npm.cmd run build
Push-Location src-tauri
cargo test
cargo fmt --check
cargo check
Pop-Location
npm.cmd run tauri build
npm.cmd run release:verify
```

Birincil Windows paketi current-user NSIS installer'dır. Üretim çıktıları `src-tauri/target/release/bundle/nsis/` altında oluşur. Installer uygulama dosyalarını günceller; AppData veya lisans dosyaları için özel silme kuralı yoktur.

## Sürüm ve signed updater

Tek kanonik uygulama sürümü `package.json` içindedir. `src-tauri/tauri.conf.json` sürümü doğrudan bu dosyadan okur; Cargo paket sürümü release öncesinde aynı değere yükseltilir.

Updater, lisans Ed25519 anahtarlarından ayrı bir Tauri minisign anahtar çifti kullanır. Public updater anahtarı Tauri yapılandırmasındadır. Private anahtar ve parolası yalnız yerel secret deposunda veya GitHub Actions secrets içinde tutulur.

Yerel signed build için resmi değişkenler:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -LiteralPath "<repo-dışındaki-updater-private-key-yolu>" -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "<updater-key-password>"
npm.cmd run tauri build
```

GitHub Actions secrets:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Kararlı updater endpoint'i `https://github.com/elorstam/arz-predictor/releases/latest/download/latest.json` adresidir. `v*` etiketi `.github/workflows/release.yml` workflow'unu başlatır; workflow aynı endpoint'i repository bilgisinden build'e uygular, NSIS installer/signature üretir ve kararlı GitHub Release'a yükler. Uygulama yalnız daha yeni sürümü sunar; geçersiz veya imzasız paket native Tauri updater tarafından reddedilir.

Yayın ve geri alma adımları için `RELEASE_CHECKLIST.md` dosyasını kullanın.
