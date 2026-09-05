# ARZ Predictor Release Checklist

## 1. Sürüm

- `package.json` sürümünü semver olarak yükseltin.
- `src-tauri/Cargo.toml` paket sürümünü aynı değere yükseltin.
- `src-tauri/tauri.conf.json` içindeki `../package.json` sürüm kaynağını değiştirmeyin.
- Release tag'inin tam olarak `v<version>` olduğundan emin olun.

## 2. Yerel doğrulama

```powershell
npm.cmd install
npm.cmd test
npm.cmd run build
Push-Location src-tauri
cargo test
cargo fmt --check
cargo check
Pop-Location
```

Geçerli Phase 13 AppData lisans durumunu yedekleyin; ham lisans anahtarını hiçbir release dosyasına koymayın.

## 3. Signed Windows build

Private updater key'i repository dışında tutun. `TAURI_SIGNING_PRIVATE_KEY` ve `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` değerlerini yalnız build process ortamında ayarlayın.

```powershell
npm.cmd run tauri build
npm.cmd run release:verify
cargo run --manifest-path src-tauri/Cargo.toml --example verify_updater_signature --release -- "<installer.exe>" "<installer.exe.sig>"
```

Beklenen dizin: `src-tauri/target/release/bundle/nsis/`.

## 4. GitHub Release

- Repository secrets içine `TAURI_SIGNING_PRIVATE_KEY` ve `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` ekleyin.
- Branch'te `.github/workflows/release.yml` bulunduğunu doğrulayın.
- Testler geçen commit'e `v<version>` etiketi gönderin.
- Workflow'un NSIS installer, installer `.sig` ve `latest.json` yüklediğini doğrulayın.
- Release notes içinde lisans/AppData migration etkisini açıkça belirtin.

Updater yalnız `https://github.com/elorstam/arz-predictor/releases/latest/download/latest.json` adresindeki kararlı GitHub Release metadata'sını HTTPS üzerinden okur. Beta/nightly kanalı yoktur.

## 5. Kurulum ve yükseltme kabulü

- Installer'ı current-user olarak kurun ve Start Menu kısayolunu doğrulayın.
- Kurulu uygulamayı açın; Ayarlar sürümünü ve updater bölümünü doğrulayın.
- Aynı kullanıcı/cihazdaki aktivasyon tokenının korunduğunu doğrulayın.
- Aynı cihaz online lisans doğrulamasını çalıştırın.
- Önceki sürümün üstüne yeni sürümü kurup AppData, ayarlar, cache ve cihaz bağının değişmediğini doğrulayın.

## 6. Başarısız yayın ve rollback

- Hatalı release'ı veya `latest.json` dosyasını yayınlamayın; yayımlandıysa GitHub Release'ı derhal taslağa alın.
- Aynı tag veya sürüm numarasındaki artifact'ı sessizce değiştirmeyin.
- Düzeltmeyi daha yüksek bir patch sürümüyle yeniden imzalayıp yayınlayın.
- Eski private updater key ile imzalanmamış paketleri dağıtmayın. Anahtar kaybı/rotasyonu ayrı bir istemci public-key geçiş sürümü gerektirir.
- Installer kaldırması için AppData silen özel hook eklemeyin.
