//! Verify a signed runtime update index against a public key.
//!
//! Usage:
//! `cargo run --quiet -p anvil --example verify_runtime_index <index.json> <public-key-hex>`
//!
//! Prints the payload JSON when the signature is valid, so release
//! automation and key rotations can be checked locally with the same
//! verification the serve self-updater performs.

use ring::signature::UnparsedPublicKey;

fn main() {
  let mut arguments = std::env::args();
  let _ = arguments.next();
  let (Some(path), Some(public_key_hex), None) =
    (arguments.next(), arguments.next(), arguments.next())
  else {
    eprintln!("usage: verify_runtime_index <index.json> <public-key-hex>");
    std::process::exit(2);
  };
  let source = std::fs::read(&path).unwrap_or_else(|error| {
    eprintln!("could not read {path}: {error}");
    std::process::exit(1);
  });
  let public_key = decode_hex(&public_key_hex).unwrap_or_else(|| {
    eprintln!("the public key must be 64 hex characters");
    std::process::exit(2);
  });

  #[derive(serde::Deserialize)]
  #[serde(deny_unknown_fields)]
  struct Envelope {
    payload: String,
    signature: String,
  }
  let envelope: Envelope = serde_json::from_slice(&source).expect("invalid envelope JSON");
  let payload = decode_hex(&envelope.payload).expect("payload is not hex");
  let signature = decode_hex(&envelope.signature).expect("signature is not hex");
  UnparsedPublicKey::new(&ring::signature::ED25519, &public_key)
    .verify(&payload, &signature)
    .unwrap_or_else(|error| {
      eprintln!("signature verification failed: {error}");
      std::process::exit(1);
    });
  let payload: serde_json::Value = serde_json::from_slice(&payload).expect("invalid payload JSON");
  println!("{}", serde_json::to_string_pretty(&payload).unwrap());
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
  (value.len().is_multiple_of(2) && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
    .then(|| {
      (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()
    })
    .flatten()
}
