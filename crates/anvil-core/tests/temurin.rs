use super::*;
#[test]
fn major_and_exact_versions_have_distinct_resolution() {
  let major = VersionRequest::parse("21").unwrap();
  assert_eq!(major.major, 21);
  assert!(major.exact.is_none());
  let exact = VersionRequest::parse("21.0.8+9").unwrap();
  assert_eq!(exact.exact.as_deref(), Some("21.0.8+9"));
  assert_eq!(exact.range.as_deref(), Some("[21.0.8,21.0.9)"));
  assert_eq!(
    VersionRequest::parse("21.0.12.1+1")
      .unwrap()
      .range
      .as_deref(),
    Some("[21.0.12.1,21.0.12.2)")
  );
  assert!(VersionRequest::parse("8").is_ok());
}
#[test]
fn rejects_malformed_or_unbounded_versions() {
  for input in [
    "",
    "7",
    "100",
    "21.0.8",
    "21.0.8+",
    "21.0.8+9/../../",
    "8u462",
    "21.0.4294967295+9",
    "9999999999999999999999",
    "21..8+9",
  ] {
    assert!(VersionRequest::parse(input).is_err(), "{input}");
  }
}
