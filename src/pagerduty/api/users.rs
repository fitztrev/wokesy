use serde::Deserialize;

use crate::pagerduty::{User, UserId};
use crate::people::{Email, Name};

use super::{Api, ApiError, Entry, id_of};

#[derive(Deserialize)]
struct UsersAnswer {
    users: Vec<Entry<UserAnswer>>,
}

#[derive(Deserialize)]
struct UserAnswer {
    id: UserId,
    name: Name,
    email: Email,
}

impl From<UserAnswer> for User {
    fn from(answer: UserAnswer) -> Self {
        Self {
            id: answer.id,
            name: answer.name,
            email: answer.email,
        }
    }
}

impl Api {
    pub async fn users(&self) -> Result<Vec<User>, ApiError> {
        let pages: Vec<UsersAnswer> = self.every_page(&["users"], &[]).await?;
        Ok(pages
            .into_iter()
            .flat_map(|page| page.users)
            .filter_map(|user| {
                user.read_or_else(|unreadable| {
                    tracing::warn!(
                        user = id_of(unreadable),
                        "pagerduty user that cannot be read skipped"
                    );
                })
            })
            .map(User::from)
            .collect())
    }
}
