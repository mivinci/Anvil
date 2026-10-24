use crate::{Error, Result};

#[derive(Clone, Copy)]
pub(crate) struct Platform {
  pub os: &'static str,
  pub arch: &'static str,
}

impl Platform {
  pub fn current() -> Result<Self> {
    let os = match std::env::consts::OS {
      "linux" => "linux",
      "macos" => "mac",
      "windows" => "windows",
      other => return Err(Error::Unsupported(other.into())),
    };
    if cfg!(target_env = "musl") {
      return Err(Error::Unsupported(
        "musl Linux (Temurin glibc packages cannot be used)".into(),
      ));
    }
    let arch = match std::env::consts::ARCH {
      "x86_64" => "x64",
      "aarch64" => "aarch64",
      other => return Err(Error::Unsupported(other.into())),
    };
    Ok(Self { os, arch })
  }
}
