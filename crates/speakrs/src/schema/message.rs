use chrono::{DateTime, Utc};
use speakrs_storage::{
    key::UuidKey,
    pagination::{Edge, Page, Pagination},
    tree::{TreeError, TreeResult, Tx},
};

use crate::{
    common::lens::Lens,
    schema::{ChannelId, DataStore, IdNotFound, LensResult, UserId, user::User},
};

pub type MessageId = UuidKey<Message>;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub timestamp: DateTime<Utc>,
    pub author: UserId,
    pub content: String,
    pub channel: ChannelId,
}

impl Message {
    /// Create MessageData with timestamp now
    pub fn now(author: UserId, channel: ChannelId, content: String) -> Self {
        Self::new(Utc::now(), channel, author, content)
    }

    pub fn new(
        timestamp: DateTime<Utc>,
        channel: ChannelId,
        author: UserId,
        content: String,
    ) -> Self {
        Self {
            timestamp,
            author,
            content,
            channel,
        }
    }
}

impl<S> DataStore<S> {
    pub fn add_message(&self, data: Message) -> TreeResult<MessageId> {
        Ok(Tx((
            &self.messages,
            &self.messages_by_channel,
            &self.messages_by_author,
        ))
        .transaction(|(tx, by_channel, by_author)| {
            let id = self.messages.transact_add(tx, &data)?;
            by_channel.insert_relation(data.channel, id)?;
            by_author.insert_relation(data.author, id)?;
            Ok(id)
        })?)
    }

    pub fn sync_message(&self, data: Edge<Message>) -> TreeResult<MessageId> {
        Ok(Tx((
            &self.messages,
            &self.messages_by_channel,
            &self.messages_by_author,
        ))
        .transaction(|(tx, by_channel, by_author)| {
            let id = self.messages.transact_insert(tx, data.cursor, &data.node)?;
            by_channel.insert_relation(data.channel, id)?;
            by_author.insert_relation(data.author, id)?;
            Ok(id)
        })?)
    }

    #[allow(unused)]
    pub fn remove_message(&self, id: MessageId) -> TreeResult<Message> {
        Ok(Tx((
            &self.messages,
            &self.messages_by_channel,
            &self.messages_by_author,
        ))
        .transaction(|(tx, by_channel, by_author)| {
            let removed = tx
                .remove(id)?
                .ok_or(IdNotFound(id))?
                .decode()
                .map_err(TreeError::other)?;
            by_channel.remove_relation(removed.channel, id)?;
            by_author.remove_relation(removed.author, id)?;
            Ok(removed)
        })?)
    }

    pub fn try_message(&self, id: MessageId) -> LensResult<'_, Option<Edge<Message>>, S> {
        Ok(self.lens(self.messages.get_edge(id)?))
    }

    pub fn message(&self, id: MessageId) -> LensResult<'_, Edge<Message>, S> {
        Ok(self
            .try_message(id)?
            .map_lens(|opt, _| opt.ok_or(IdNotFound(id)))?)
    }

    pub fn messages(
        &self,
        pagination: Pagination<MessageId>,
    ) -> LensResult<'_, Page<Message, MessageId>, S> {
        Ok(self.lens(self.messages.page(pagination)?))
    }
}

impl<'db, S> Lens<'db, Edge<Message>, S> {
    #[allow(unused)] // TODO
    pub fn author(&self) -> LensResult<'db, Edge<User>, S> {
        self.store.user(self.author)
    }
}
