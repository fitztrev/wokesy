use nutype::nutype;

use crate::people::{Email, Name};

#[nutype(
    validate(not_empty),
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
pub struct UserId(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub name: Name,
    pub email: Email,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserRef {
    pub id: UserId,
    pub name: Name,
}
