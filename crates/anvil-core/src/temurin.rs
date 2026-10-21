use q2q_http::{Checksum, Executor};
use serde::Deserialize;
use url::Url;

use crate::{Error, Result, platform::Platform};

const METADATA_LIMIT: u64 = 2 * 1024 * 1024;

pub(crate) struct VersionRequest {
  pub major: u32,
  exact: Option<String>,
  range: Option<String>,
}

impl VersionRequest {
  pub fn parse(input: &str) -> Result<Self> {
    let invalid = || {
      Error::Invalid(
        "Java version must be a major (21) or exact release with build (21.0.8+9)".into(),
      )
    };
    if input.is_empty() || input.len() > 64 {
      return Err(invalid());
    }
    if input.bytes().all(|b| b.is_ascii_digit()) {
      let major = input.parse::<u32>().map_err(|_| invalid())?;
      if !(8..=99).contains(&major) {
        return Err(invalid());
      }
      return Ok(Self {
        major,
        exact: None,
        range: None,
      });
    }
    let (base, build) = input.split_once('+').ok_or_else(invalid)?;
    let numbers = base
      .split('.')
      .map(str::parse::<u32>)
      .collect::<std::result::Result<Vec<_>, _>>()
      .map_err(|_| invalid())?;
    if !(3..=4).contains(&numbers.len())
      || !(9..=99).contains(&numbers[0])
      || build.is_empty()
      || !build.bytes().all(|b| b.is_ascii_digit())
      || build.parse::<u32>().is_err()
    {
      return Err(invalid());
    }
    let mut upper = numbers.clone();
    let last = upper.last_mut().unwrap();
    *last = last.checked_add(1).ok_or_else(invalid)?;
    let upper = upper
      .iter()
      .map(u32::to_string)
      .collect::<Vec<_>>()
      .join(".");
    Ok(Self {
      major: numbers[0],
      exact: Some(input.into()),
      range: Some(format!("[{base},{upper})")),
    })
  }
}

#[derive(Deserialize)]
struct Version {
  major: u32,
  openjdk_version: String,
}
#[derive(Deserialize)]
struct Binary {
  architecture: String,
  os: String,
  image_type: String,
  jvm_impl: String,
  package: Package,
}
#[derive(Deserialize)]
struct Package {
  checksum: String,
  link: String,
  name: String,
  size: u64,
}
#[derive(Deserialize)]
struct Latest {
  binary: Binary,
  version: Version,
  vendor: String,
}
#[derive(Deserialize)]
struct Release {
  binaries: Vec<Binary>,
  version_data: Version,
  vendor: String,
}

pub(crate) struct Resolved {
  pub version: String,
  pub major: u32,
  pub image: String,
  pub checksum: String,
  pub url: String,
  pub size: u64,
  pub zip: bool,
}

/// Prefer JRE; fall back to JDK only when the provider has no matching JRE.
pub(crate) async fn resolve(
  http: &Executor,
  request: &VersionRequest,
  platform: Platform,
) -> Result<Resolved> {
  for image in ["jre", "jdk"] {
    // Adoptium applies range syntax in a path segment. URL serializers leave
    // brackets and commas literal here, but the API requires percent encoding.
    let path = if let Some(range) = &request.range {
      format!(
        "/v3/assets/version/{}",
        range
          .replace('[', "%5B")
          .replace(',', "%2C")
          .replace(')', "%29")
      )
    } else {
      format!("/v3/assets/latest/{}/hotspot", request.major)
    };
    let mut url = Url::parse(&format!("https://api.adoptium.net{path}")).unwrap();
    url.query_pairs_mut().extend_pairs([
      ("architecture", platform.arch),
      ("os", platform.os),
      ("image_type", image),
      ("vendor", "eclipse"),
      ("jvm_impl", "hotspot"),
      ("heap_size", "normal"),
      ("project", "jdk"),
      ("release_type", "ga"),
      ("page_size", "20"),
      ("sort_order", "DESC"),
    ]);
    let response = match http
      .get(url.as_str())
      .max_body_size(METADATA_LIMIT)
      .send()
      .await
    {
      Ok(response) => response,
      Err(q2q_http::Error::Status { code: 404, .. }) => continue,
      Err(e) => return Err(e.into()),
    };
    let bytes = response.collect_bounded(METADATA_LIMIT as usize).await?;
    let candidates = if request.exact.is_some() {
      let releases: Vec<Release> = serde_json::from_slice(&bytes)?;
      releases
        .into_iter()
        .flat_map(|r| {
          r.binaries.into_iter().map(move |b| {
            (
              r.vendor.clone(),
              r.version_data.major,
              r.version_data.openjdk_version.clone(),
              b,
            )
          })
        })
        .collect::<Vec<_>>()
    } else {
      let releases: Vec<Latest> = serde_json::from_slice(&bytes)?;
      releases
        .into_iter()
        .map(|r| {
          (
            r.vendor,
            r.version.major,
            r.version.openjdk_version,
            r.binary,
          )
        })
        .collect()
    };
    for (vendor, major, version, binary) in candidates {
      let version = version.strip_suffix("-LTS").unwrap_or(&version).to_owned();
      if vendor != "eclipse"
        || major != request.major
        || binary.os != platform.os
        || binary.architecture != platform.arch
        || binary.image_type != image
        || binary.jvm_impl != "hotspot"
        || request.exact.as_ref().is_some_and(|v| v != &version)
      {
        continue;
      }
      if version.len() > 64
        || !version
          .bytes()
          .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
      {
        return Err(Error::Metadata("invalid release version".into()));
      }
      let p = binary.package;
      let link = Url::parse(&p.link).map_err(|_| Error::Metadata("invalid download URL".into()))?;
      if link.scheme() != "https"
        || link.host_str().is_none()
        || !link.username().is_empty()
        || link.password().is_some()
        || p.size == 0
        || p.size > 1024 * 1024 * 1024
      {
        return Err(Error::Metadata("invalid package URL or size".into()));
      }
      Checksum::sha256(&p.checksum)?;
      let zip = match platform.os {
        "windows" if p.name.ends_with(".zip") => true,
        "linux" | "mac" if p.name.ends_with(".tar.gz") => false,
        _ => return Err(Error::Metadata("unsupported package format".into())),
      };
      return Ok(Resolved {
        version,
        major,
        image: image.into(),
        checksum: p.checksum.to_ascii_lowercase(),
        url: p.link,
        size: p.size,
        zip,
      });
    }
  }
  Err(Error::NotFound(format!(
    "Java {} on {}-{}",
    request
      .exact
      .as_deref()
      .unwrap_or(&request.major.to_string()),
    platform.os,
    platform.arch
  )))
}

#[cfg(test)]
#[path = "../tests/unit/temurin.rs"]
mod tests;
