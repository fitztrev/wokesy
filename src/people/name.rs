use nutype::nutype;

/// ```
/// use wokesy::people::Name;
///
/// assert_eq!(Name::try_new("  Alice Example ").unwrap().as_str(), "Alice Example");
/// assert!(Name::try_new("  ").is_err());
/// ```
#[nutype(
    sanitize(trim),
    validate(not_empty),
    derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deref, Deserialize)
)]
pub struct Name(String);
