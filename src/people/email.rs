use nutype::nutype;

/// ```
/// use wokesy::people::Email;
///
/// assert_eq!(Email::try_new("  Alice@Example.COM ").unwrap().as_str(), "alice@example.com");
/// assert!(Email::try_new("alice").is_err());
/// assert!(Email::try_new("@example.com").is_err());
/// assert!(Email::try_new("alice@").is_err());
/// assert!(Email::try_new("alice@example@com").is_err());
/// ```
#[nutype(
    sanitize(trim, lowercase),
    validate(predicate = has_a_local_part_and_a_domain),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Deref,
        Display,
        Deserialize
    )
)]
pub struct Email(String);

fn has_a_local_part_and_a_domain(candidate: &str) -> bool {
    candidate.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty() && !domain.is_empty() && !domain.contains('@')
    })
}
