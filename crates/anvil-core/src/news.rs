//! Bounded reader for the public Minecraft.net news feed.
use crate::{
  Context, Error, Result,
  catalog_cache::{self, NEWS},
};
use quick_xml::de::from_str;
use reqwest::{Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

const FEED_URL: &str = "https://www.minecraft.net/en-us/feeds/community-content/rss";
const MAX_FEED_BYTES: usize = 1024 * 1024;
const MAX_ITEMS: usize = 24;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewsItem {
  pub title: String,
  pub url: String,
  pub published_at: Option<String>,
  pub categories: Vec<String>,
}

pub struct CachedNews {
  pub items: Vec<NewsItem>,
  pub fetched_at: SystemTime,
  pub persistence_error: Option<Error>,
}

#[derive(Deserialize)]
struct RssDocument {
  channel: RssChannel,
}

#[derive(Deserialize)]
struct RssChannel {
  #[serde(default)]
  item: Vec<RssItem>,
}

#[derive(Deserialize)]
struct RssItem {
  title: Option<String>,
  link: Option<RssLink>,
  #[serde(rename = "pubDate")]
  pub_date: Option<String>,
  #[serde(default)]
  category: Vec<String>,
}

#[derive(Deserialize)]
struct RssLink {
  #[serde(rename = "@href")]
  href: Option<String>,
  #[serde(rename = "$text")]
  text: Option<String>,
}

pub async fn load_cached_news(context: &Context) -> Result<Option<CachedNews>> {
  let Some(snapshot) = catalog_cache::load::<Vec<NewsItem>>(context, NEWS, "minecraft").await?
  else {
    return Ok(None);
  };
  Ok(Some(CachedNews {
    items: snapshot.value,
    fetched_at: snapshot.fetched_at,
    persistence_error: None,
  }))
}

pub async fn refresh_news(context: &Context) -> Result<CachedNews> {
  let client = Client::builder()
    .timeout(Duration::from_secs(12))
    .redirect(Policy::limited(3))
    .build()
    .map_err(|_| Error::Metadata("无法准备官方新闻连接".into()))?;
  let mut response = client
    .get(FEED_URL)
    .header(
      reqwest::header::ACCEPT,
      "application/rss+xml, application/xml, text/xml",
    )
    .header(reqwest::header::USER_AGENT, "Anvil desktop news reader")
    .send()
    .await
    .map_err(|_| Error::Metadata("无法连接 Minecraft 官方新闻源".into()))?;
  if !response.status().is_success() {
    return Err(Error::Metadata(format!(
      "Minecraft 官方新闻源返回 HTTP {}",
      response.status().as_u16()
    )));
  }
  let final_url = response.url();
  if final_url.scheme() != "https"
    || !matches!(
      final_url.host_str(),
      Some("minecraft.net" | "www.minecraft.net")
    )
  {
    return Err(Error::Metadata("官方新闻源跳转到了不受信任的地址".into()));
  }
  if response
    .content_length()
    .is_some_and(|length| length > MAX_FEED_BYTES as u64)
  {
    return Err(Error::Metadata("官方新闻 Feed 超出大小限制".into()));
  }
  let mut bytes = Vec::with_capacity(
    response
      .content_length()
      .unwrap_or(0)
      .min(MAX_FEED_BYTES as u64) as usize,
  );
  while let Some(chunk) = response
    .chunk()
    .await
    .map_err(|_| Error::Metadata("读取 Minecraft 官方新闻 Feed 失败".into()))?
  {
    if bytes.len().saturating_add(chunk.len()) > MAX_FEED_BYTES {
      return Err(Error::Metadata("官方新闻 Feed 超出大小限制".into()));
    }
    bytes.extend_from_slice(&chunk);
  }
  let xml = decode_feed(
    &bytes,
    response.headers().get(reqwest::header::CONTENT_TYPE),
  )?;
  let feed: RssDocument =
    from_str(&xml).map_err(|_| Error::Metadata("Minecraft 官方新闻 Feed 格式无效".into()))?;
  let mut items = Vec::new();
  for item in feed.channel.item.into_iter().take(MAX_ITEMS * 2) {
    let Some(title) = item.title.map(|value| value.trim().to_owned()) else {
      continue;
    };
    let Some(url) = item
      .link
      .and_then(|link| link.href.or(link.text))
      .map(|value| value.trim().to_owned())
    else {
      continue;
    };
    if title.is_empty() || title.len() > 300 || !valid_article_url(&url) {
      continue;
    }
    let categories = item
      .category
      .into_iter()
      .map(|value| value.trim().to_owned())
      .filter(|value| !value.is_empty() && value.len() <= 80)
      .take(8)
      .collect();
    items.push(NewsItem {
      title,
      url,
      published_at: item
        .pub_date
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty() && value.len() <= 128),
      categories,
    });
    if items.len() == MAX_ITEMS {
      break;
    }
  }
  if items.is_empty() {
    return Err(Error::Metadata("官方新闻 Feed 中没有可显示的文章".into()));
  }
  let fetched_at = SystemTime::now();
  let persistence_error =
    catalog_cache::save(context, NEWS, "minecraft", items.clone(), fetched_at)
      .await
      .err();
  Ok(CachedNews {
    items,
    fetched_at,
    persistence_error,
  })
}

fn decode_feed(
  bytes: &[u8],
  content_type: Option<&reqwest::header::HeaderValue>,
) -> Result<String> {
  let charset = content_type
    .and_then(|value| value.to_str().ok())
    .unwrap_or_default()
    .to_ascii_lowercase();

  let (payload, little_endian) = if let Some(payload) = bytes.strip_prefix(&[0xff, 0xfe]) {
    (Some(payload), Some(true))
  } else if let Some(payload) = bytes.strip_prefix(&[0xfe, 0xff]) {
    (Some(payload), Some(false))
  } else if bytes.starts_with(&[b'<', 0, b'?', 0]) {
    (Some(bytes), Some(true))
  } else if bytes.starts_with(&[0, b'<', 0, b'?']) {
    (Some(bytes), Some(false))
  } else if charset.contains("utf-16") {
    return Err(Error::Metadata("UTF-16 RSS Feed 缺少可识别的字节序".into()));
  } else {
    (None, None)
  };

  if let (Some(payload), Some(little_endian)) = (payload, little_endian) {
    if payload.len() % 2 != 0 {
      return Err(Error::Metadata("UTF-16 RSS Feed 长度无效".into()));
    }
    let units: Vec<u16> = payload
      .as_chunks::<2>()
      .0
      .iter()
      .map(|pair| {
        if little_endian {
          u16::from_le_bytes([pair[0], pair[1]])
        } else {
          u16::from_be_bytes([pair[0], pair[1]])
        }
      })
      .collect();
    return String::from_utf16(&units)
      .map_err(|_| Error::Metadata("UTF-16 RSS Feed 编码无效".into()));
  }

  let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
  std::str::from_utf8(bytes)
    .map(str::to_owned)
    .map_err(|_| Error::Metadata("RSS Feed 不是有效的 UTF-8".into()))
}

fn valid_article_url(value: &str) -> bool {
  value.len() <= 2048
    && url::Url::parse(value).ok().is_some_and(|url| {
      url.scheme() == "https"
        && url.host_str().is_some_and(|host| {
          host == "minecraft.net" || host.ends_with(".minecraft.net") || host == "aka.ms"
        })
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
    })
}
