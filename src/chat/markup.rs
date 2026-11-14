use time::{OffsetDateTime, UtcOffset};

use super::conversation::ChatUserId;

impl ChatUserId {
    /// ```
    /// use wokesy::chat::ChatUserId;
    ///
    /// assert_eq!(ChatUserId(17).mention(), "@**|17**");
    /// ```
    #[must_use]
    pub fn mention(self) -> String {
        format!("@**|{}**", self.0)
    }

    /// ```
    /// use wokesy::chat::ChatUserId;
    ///
    /// assert_eq!(ChatUserId(17).silent_mention(), "@_**|17**");
    /// ```
    #[must_use]
    pub fn silent_mention(self) -> String {
        format!("@_**|{}**", self.0)
    }
}

/// ```
/// use wokesy::chat::time_tag;
/// use time::macros::datetime;
///
/// assert_eq!(time_tag(datetime!(2026-11-14 20:00 UTC)), "<time:2026-11-14T20:00:00Z>");
/// assert_eq!(time_tag(datetime!(2026-11-14 21:30:15.75 +01:30)), "<time:2026-11-14T20:00:15Z>");
/// assert_eq!(time_tag(datetime!(2026-01-01 03:04:05 +05:00)), "<time:2025-12-31T22:04:05Z>");
/// ```
#[must_use]
pub fn time_tag(at: OffsetDateTime) -> String {
    let utc = at.to_offset(UtcOffset::UTC);
    format!(
        "<time:{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z>",
        utc.year(),
        u8::from(utc.month()),
        utc.day(),
        utc.hour(),
        utc.minute(),
        utc.second(),
    )
}
