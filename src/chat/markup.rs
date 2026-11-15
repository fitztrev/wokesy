use time::{OffsetDateTime, UtcOffset};

use super::conversation::ChatUserId;

const MENTION: &str = "@*";
const SILENT_MENTION: &str = "@_*";

impl ChatUserId {
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
/// use wokesy::chat::silenced;
///
/// assert_eq!(silenced("ask @**Carol Example**"), "ask @_**Carol Example**");
/// assert_eq!(silenced(">@**all** and @*sysadmins*"), ">@_**all** and @_*sysadmins*");
/// assert_eq!(silenced("@_**Carol Example|17** @_*sysadmins*"), "@_**Carol Example|17** @_*sysadmins*");
/// assert_eq!(silenced("@@**all**"), "@@_**all**");
/// assert_eq!(silenced("alice@example.com *is* down"), "alice@example.com *is* down");
/// ```
#[must_use]
pub fn silenced(text: &str) -> String {
    text.replace(MENTION, SILENT_MENTION)
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
