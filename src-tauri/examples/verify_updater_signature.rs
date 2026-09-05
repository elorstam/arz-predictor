use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};
use serde_json::Value;
use std::{env, fs, path::PathBuf};

fn main() {
    let mut args = env::args_os().skip(1);
    let artifact = PathBuf::from(args.next().expect("artifact path is required"));
    let signature_path = PathBuf::from(args.next().expect("signature path is required"));
    assert!(
        args.next().is_none(),
        "expected artifact and signature paths only"
    );

    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let config: Value = serde_json::from_slice(&fs::read(config_path).expect("read Tauri config"))
        .expect("parse Tauri config");
    let encoded_public_key = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .expect("configured updater public key");
    let public_key_text = String::from_utf8(
        STANDARD
            .decode(encoded_public_key)
            .expect("decode configured updater public key"),
    )
    .expect("public key is UTF-8");
    let public_key = PublicKey::decode(&public_key_text).expect("parse updater public key");
    let encoded_signature = fs::read_to_string(signature_path).expect("read updater signature");
    let signature_text = String::from_utf8(
        STANDARD
            .decode(encoded_signature.trim())
            .expect("decode Tauri updater signature envelope"),
    )
    .expect("updater signature is UTF-8");
    let signature = Signature::decode(&signature_text).expect("parse updater signature");
    let artifact_bytes = fs::read(artifact).expect("read updater artifact");

    public_key
        .verify(&artifact_bytes, &signature, false)
        .expect("valid updater artifact must verify");

    let mut tampered = artifact_bytes;
    let first = tampered.first_mut().expect("updater artifact is not empty");
    *first ^= 1;
    assert!(
        public_key.verify(&tampered, &signature, false).is_err(),
        "tampered updater artifact must be rejected"
    );

    println!("valid updater signature accepted; tampered artifact rejected");
}
