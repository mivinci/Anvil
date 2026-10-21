fn main() {
  if std::env::var_os("CARGO_FEATURE_DESKTOP").is_some() {
    slint_build::compile("ui/main.slint").expect("failed to compile Anvil UI");
  }
}
