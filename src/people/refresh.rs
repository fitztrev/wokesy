use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

use super::config::Config;
use super::directory::Directory;
use super::links::Links;
use super::members::{ChatMembers, MembersError};
use crate::pagerduty;

#[derive(Debug, thiserror::Error)]
enum ReadError {
    #[error("could not read the pagerduty users")]
    PagerDuty(#[from] pagerduty::ApiError),
    #[error(transparent)]
    Chat(#[from] MembersError),
}

pub async fn keep_matched<C: ChatMembers>(
    pagerduty: pagerduty::Api,
    chat: C,
    config: Config,
    directory: watch::Sender<Option<Directory>>,
) {
    let mut refreshes = tokio::time::interval(config.refresh_interval);
    refreshes.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            () = directory.closed() => return,
            _ = refreshes.tick() => {}
        }
        match matched(&pagerduty, &chat, &config.links).await {
            Ok(matched) => publish(&directory, matched),
            Err(error) => {
                tracing::warn!(error = %error, cause = ?std::error::Error::source(&error), "keeping the previous directory");
            }
        }
    }
}

async fn matched<C: ChatMembers>(
    pagerduty: &pagerduty::Api,
    chat: &C,
    links: &Links,
) -> Result<Directory, ReadError> {
    let users = pagerduty.users().await?;
    let members = chat.members().await?;
    tracing::debug!(
        users = users.len(),
        members = members.len(),
        "read the pagerduty users and the chat members"
    );
    Ok(Directory::matching(&users, &members, links))
}

fn publish(directory: &watch::Sender<Option<Directory>>, matched: Directory) {
    let changed = directory.send_if_modified(|published| {
        if published.as_ref() == Some(&matched) {
            return false;
        }
        for one in matched.matched_since(published.as_ref()) {
            tracing::info!(
                pagerduty_user = %one.user,
                chat_user = one.chat_user.0,
                rule = %one.by,
                "matched a pagerduty user to a chat user"
            );
        }
        *published = Some(matched);
        true
    });
    if changed {
        tracing::info!("people matched between pagerduty and chat");
    }
}

#[cfg(test)]
mod tests;
