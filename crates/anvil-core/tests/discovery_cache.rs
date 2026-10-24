//! Offline public discovery snapshots. No platform requests or credentials.
use super::*;
use crate::{Paths, curseforge as cf, modrinth as mr};
use serde_json::json;
use std::path::Path;

fn context(root: &Path) -> Context {
  Context::new(Paths::discover(Some(root.to_owned())).unwrap()).unwrap()
}
fn mr_project(id: &str) -> mr::Project {
  serde_json::from_value(json!({"id":id,"name":"Fixture","summary":"Offline project","authors":[{"name":"Fixture author"}],"links":{"websiteUrl":format!("https://modrinth.com/mod/{id}")}})).unwrap()
}
fn cf_project(id: u32) -> cf::Project {
  serde_json::from_value(json!({"id":id,"gameId":432,"classId":6,"name":"Fixture","summary":"Offline project","authors":[],"links":{"websiteUrl":"https://www.curseforge.com/minecraft/mc-mods/fixture"},"allowModDistribution":true})).unwrap()
}

#[tokio::test]
async fn discovery_icons_round_trip_and_reject_untrusted_urls() {
  let root = tempfile::tempdir().unwrap();
  let ctx = context(root.path());
  let at = SystemTime::now();
  let mr_query = mr::DiscoveryQuery::Project {
    project: "abcdefgh".into(),
  };
  let cf_query = cf::DiscoveryQuery::Project { project: 42 };
  assert!(mr_project("abcdefgh").icon_url.is_none());
  assert!(cf_project(42).logo.is_none());
  assert!(mr_project("abcdefgh").download_count.is_none());
  assert!(mr_project("abcdefgh").updated_at.is_none());
  assert!(cf_project(42).download_count.is_none());
  assert!(cf_project(42).date_modified.is_none());
  for (value, valid) in [
    ("https://cdn.modrinth.com/data/abcdefgh/icon.png", true),
    ("http://cdn.modrinth.com/icon.png", false),
    ("https://cdn.modrinth.com.evil.invalid/icon.png", false),
    ("https://user@cdn.modrinth.com/icon.png", false),
    ("https://cdn.modrinth.com:8443/icon.png", false),
  ] {
    let mut project = mr_project("abcdefgh");
    project.icon_url = Some(value.into());
    project.download_count = Some(12500);
    project.updated_at = Some("2026-10-24T12:30:00Z".into());
    save(
      &ctx,
      MODRINTH,
      &mr_query.identity().unwrap(),
      mr::DiscoveryPage::Project(project),
      at,
    )
    .await
    .unwrap();
    let result = mr_query.load_cached(&ctx).await;
    assert_eq!(result.is_ok(), valid, "{value}");
    if valid {
      let mr::DiscoveryPage::Project(project) = result.unwrap().unwrap().page else {
        panic!("wrong kind")
      };
      assert_eq!(project.icon_url.as_deref(), Some(value));
      assert_eq!(project.download_count, Some(12500));
      assert_eq!(project.updated_at.as_deref(), Some("2026-10-24T12:30:00Z"));
    }
  }
  for (value, valid) in [
    (
      "https://media.forgecdn.net/avatars/thumbnails/1/2/icon.png",
      true,
    ),
    ("https://edge.forgecdn.net/avatars/1/2/icon.png", true),
    ("http://media.forgecdn.net/icon.png", false),
    ("https://media.forgecdn.net.evil.invalid/icon.png", false),
    ("https://user@media.forgecdn.net/icon.png", false),
    ("https://media.forgecdn.net:8443/icon.png", false),
  ] {
    let mut project = cf_project(42);
    project.logo = Some(cf::ProjectLogo {
      thumbnail_url: Some(value.into()),
      url: None,
    });
    project.download_count = Some(250000000);
    project.date_modified = Some("2026-10-24T12:30:00Z".into());
    save(
      &ctx,
      CURSEFORGE,
      &cf_query.identity().unwrap(),
      cf::DiscoveryPage::Project(project),
      at,
    )
    .await
    .unwrap();
    let result = cf_query.load_cached(&ctx).await;
    assert_eq!(result.is_ok(), valid, "{value}");
    if valid {
      let cf::DiscoveryPage::Project(project) = result.unwrap().unwrap().page else {
        panic!("wrong kind")
      };
      assert_eq!(project.logo.unwrap().thumbnail_url.as_deref(), Some(value));
      assert_eq!(project.download_count, Some(250000000));
      assert_eq!(
        project.date_modified.as_deref(),
        Some("2026-10-24T12:30:00Z")
      );
    }
  }
}

#[test]
fn discovery_keys_preserve_existing_filtered_search_and_separate_global_scope() {
  let mr_filtered = mr::DiscoveryQuery::Search {
    game: Some("1.21.1".into()),
    text: "fixture".into(),
    index: 0,
  };
  let cf_filtered = cf::DiscoveryQuery::Search {
    game: Some("1.21.1".into()),
    text: "fixture".into(),
    index: 0,
  };
  let old = r#"{"kind":"search","game":"1.21.1","text":"fixture","index":0}"#;
  assert_eq!(mr_filtered.identity().unwrap(), old);
  assert_eq!(cf_filtered.identity().unwrap(), old);
  let mr_global = mr::DiscoveryQuery::Search {
    game: None,
    text: "fixture".into(),
    index: 0,
  };
  let cf_global = cf::DiscoveryQuery::Search {
    game: None,
    text: "fixture".into(),
    index: 0,
  };
  assert_ne!(mr_global.identity().unwrap(), old);
  assert_ne!(cf_global.identity().unwrap(), old);
  assert!(
    mr::DiscoveryQuery::Search {
      game: Some(String::new()),
      text: String::new(),
      index: 0
    }
    .identity()
    .is_err()
  );
  assert!(
    cf::DiscoveryQuery::Search {
      game: Some("../escape".into()),
      text: String::new(),
      index: 0
    }
    .identity()
    .is_err()
  );
  assert!(
    mr::DiscoveryQuery::Project {
      project: "../escape".into()
    }
    .identity()
    .is_err()
  );
  assert!(
    cf::DiscoveryQuery::Project { project: 0 }
      .identity()
      .is_err()
  );
}

#[tokio::test]
async fn public_search_and_project_snapshots_restore_original_time_without_instance() {
  let root = tempfile::tempdir().unwrap();
  let ctx = context(root.path());
  let at = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
  let mr_search = mr::DiscoveryQuery::Search {
    game: None,
    text: "fixture".into(),
    index: 0,
  };
  let cf_search = cf::DiscoveryQuery::Search {
    game: None,
    text: "fixture".into(),
    index: 0,
  };
  save(
    &ctx,
    MODRINTH,
    &mr_search.identity().unwrap(),
    mr::DiscoveryPage::Search(mr::Page {
      items: vec![mr_project("abcdefgh")],
      next_index: None,
    }),
    at,
  )
  .await
  .unwrap();
  save(
    &ctx,
    CURSEFORGE,
    &cf_search.identity().unwrap(),
    cf::DiscoveryPage::Search(cf::CatalogPage {
      items: vec![cf_project(42)],
      next_index: None,
    }),
    at,
  )
  .await
  .unwrap();
  let mr_detail = mr::DiscoveryQuery::Project {
    project: "abcdefgh".into(),
  };
  let cf_detail = cf::DiscoveryQuery::Project { project: 42 };
  save(
    &ctx,
    MODRINTH,
    &mr_detail.identity().unwrap(),
    mr::DiscoveryPage::Project(mr_project("abcdefgh")),
    at,
  )
  .await
  .unwrap();
  save(
    &ctx,
    CURSEFORGE,
    &cf_detail.identity().unwrap(),
    cf::DiscoveryPage::Project(cf_project(42)),
    at,
  )
  .await
  .unwrap();
  // A new Context represents a process restart, retaining only on-disk state.
  let restored = context(root.path());
  assert_eq!(
    mr_search
      .load_cached(&restored)
      .await
      .unwrap()
      .unwrap()
      .fetched_at,
    at
  );
  assert_eq!(
    cf_search
      .load_cached(&restored)
      .await
      .unwrap()
      .unwrap()
      .fetched_at,
    at
  );
  assert_eq!(
    mr_detail
      .load_cached(&restored)
      .await
      .unwrap()
      .unwrap()
      .fetched_at,
    at
  );
  assert_eq!(
    cf_detail
      .load_cached(&restored)
      .await
      .unwrap()
      .unwrap()
      .fetched_at,
    at
  );
  assert!(!ctx.paths.data.join("instances").exists());
}

#[tokio::test]
async fn snapshots_reject_mismatched_project_identity_kind_and_external_links() {
  let root = tempfile::tempdir().unwrap();
  let ctx = context(root.path());
  let at = SystemTime::now();
  let mr_query = mr::DiscoveryQuery::Project {
    project: "abcdefgh".into(),
  };
  let key = mr_query.identity().unwrap();
  save(
    &ctx,
    MODRINTH,
    &key,
    mr::DiscoveryPage::Project(mr_project("ABCDEFGH")),
    at,
  )
  .await
  .unwrap();
  assert!(mr_query.load_cached(&ctx).await.is_err());
  save(
    &ctx,
    MODRINTH,
    &key,
    mr::DiscoveryPage::Search(mr::Page::<mr::Project> {
      items: vec![],
      next_index: None,
    }),
    at,
  )
  .await
  .unwrap();
  assert!(mr_query.load_cached(&ctx).await.is_err());
  let mut project = mr_project("abcdefgh");
  project.links.website_url = "https://example.invalid/project".into();
  save(
    &ctx,
    MODRINTH,
    &key,
    mr::DiscoveryPage::Project(project),
    at,
  )
  .await
  .unwrap();
  assert!(mr_query.load_cached(&ctx).await.is_err());
  let cf_query = cf::DiscoveryQuery::Project { project: 42 };
  let key = cf_query.identity().unwrap();
  save(
    &ctx,
    CURSEFORGE,
    &key,
    cf::DiscoveryPage::Project(cf_project(43)),
    at,
  )
  .await
  .unwrap();
  assert!(cf_query.load_cached(&ctx).await.is_err());
  let mut project = cf_project(42);
  project.links.website_url = Some("https://www.curseforge.com@evil.invalid/".into());
  save(
    &ctx,
    CURSEFORGE,
    &key,
    cf::DiscoveryPage::Project(project),
    at,
  )
  .await
  .unwrap();
  assert!(cf_query.load_cached(&ctx).await.is_err());
}

#[tokio::test]
async fn global_search_and_details_share_the_existing_sixteen_entry_disk_budget() {
  let root = tempfile::tempdir().unwrap();
  let ctx = context(root.path());
  for index in 0..20 {
    let id = format!("{index:08}");
    let query = mr::DiscoveryQuery::Project {
      project: id.clone(),
    };
    save(
      &ctx,
      MODRINTH,
      &query.identity().unwrap(),
      mr::DiscoveryPage::Project(mr_project(&id)),
      SystemTime::now(),
    )
    .await
    .unwrap();
  }
  let dir = directory(&ctx, MODRINTH, false).unwrap().unwrap();
  assert_eq!(std::fs::read_dir(dir).unwrap().count(), 16);
  assert!(
    mr::DiscoveryQuery::Project {
      project: "00000019".into()
    }
    .load_cached(&ctx)
    .await
    .unwrap()
    .is_some()
  );
}
