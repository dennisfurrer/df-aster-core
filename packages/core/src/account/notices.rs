//! Announcements sent to the account itself (`announcement/direct`, `announcement/directById`),
//! each in several languages.

use serde::Deserialize;

use crate::client::http::{ApiError, Call, Transport, read};
use crate::utils::lenient;

/// One announcement.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Announcement {
    pub id: u64,
    /// One entry a language.
    #[serde(default, deserialize_with = "lenient::list")]
    pub contents: Vec<AnnouncementContent>,
    /// `MAINTENANCE` and the like.
    #[serde(default, deserialize_with = "lenient::text")]
    pub category: String,
    /// Unix ms.
    #[serde(default)]
    pub publish_time: u64,
    /// The full announcement's page.
    #[serde(default, deserialize_with = "lenient::text")]
    pub jump_link: String,
}

impl Announcement {
    /// The text in `language` (`en`, `zh`), or else the first one.
    #[must_use]
    pub fn in_language(&self, language: &str) -> Option<&AnnouncementContent> {
        self.contents.iter().find(|content| content.language == language).or_else(|| self.contents.first())
    }
}

/// An announcement in one language.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AnnouncementContent {
    pub language: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub title: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub subtitle: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub content: String,
}

/// A page of announcements.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Announcements {
    pub total: u64,
    #[serde(default, deserialize_with = "lenient::list")]
    pub rows: Vec<Announcement>,
}

/// `GET /fapi/v3/announcement/direct`: `page` from 1, `size` 20 by default.
#[must_use]
pub fn announcements_call(page: Option<u32>, size: Option<u32>) -> Call {
    Call::get("/fapi/v3/announcement/direct").maybe("page", page).maybe("size", size).signed()
}

pub async fn announcements(
    transport: &impl Transport,
    page: Option<u32>,
    size: Option<u32>,
) -> Result<Announcements, ApiError> {
    read(transport, announcements_call(page, size)).await
}

/// `GET /fapi/v3/announcement/directById`.
#[must_use]
pub fn announcement_call(id: u64) -> Call {
    Call::get("/fapi/v3/announcement/directById").field("id", id).signed()
}

pub async fn announcement(transport: &impl Transport, id: u64) -> Result<Announcement, ApiError> {
    read(transport, announcement_call(id)).await
}

#[cfg(test)]
#[path = "tests/notices.rs"]
mod tests;
