//! One-off utility to generate the Ed25519 keypair used to sign runtime
//! update indexes.
//!
//! Run with `cargo run --quiet -p anvil --example gen_update_key` to print the
//! Runtime key pair. Pass secret and variable names as arguments to generate a
//! pair for another channel without changing the key material format.

fn main() {
  let names = std::env::args().skip(1).collect::<Vec<_>>();
  let (secret_name, public_name) = match names.as_slice() {
    [] => (
      "ANVIL_RUNTIME_SIGNING_KEY".to_owned(),
      "ANVIL_RUNTIME_UPDATE_PUBLIC_KEY".to_owned(),
    ),
    [secret, public] if valid_env_name(secret) && valid_env_name(public) => {
      (secret.clone(), public.clone())
    }
    _ => {
      eprintln!("usage: gen_update_key [SECRET_NAME PUBLIC_VARIABLE_NAME]");
      std::process::exit(2);
    }
  };

  use ring::signature::KeyPair;
  let document = ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
    .expect("Ed25519 key generation is supported by ring");
  let bytes = document.as_ref();
  // Both the attribute-free v1 (48-byte) and v2 (83-byte) Ed25519 PKCS#8
  // layouts carry the raw seed in an OCTET STRING header `04 20` at offset 14
  // followed by the 32-byte seed.
  assert!(
    bytes.len() >= 48 && bytes[0] == 0x30 && bytes[14] == 0x04 && bytes[15] == 0x20,
    "unexpected PKCS#8 layout"
  );
  let key_pair =
    ring::signature::Ed25519KeyPair::from_pkcs8(bytes).expect("generated key can be parsed back");
  println!("{secret_name}={}", hex(&bytes[16..48]));
  println!("{public_name}={}", hex(key_pair.public_key().as_ref()));
}

fn valid_env_name(name: &str) -> bool {
  !name.is_empty()
    && name
      .bytes()
      .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
