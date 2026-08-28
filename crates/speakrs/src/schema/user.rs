use speakrs_storage::{
    key::UuidKey,
    pagination::{Edge, Page, Pagination},
    tree::TreeResult,
};

use crate::{
    common::lens::Lens,
    schema::{
        DataStore, IdNotFound, LensResult,
        message::{Message, MessageId},
    },
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    // IDEA: join date, bio
    pub name: String,
}

impl User {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

pub type UserId = UuidKey<User>;

impl<S> DataStore<S> {
    #[allow(unused)]
    pub fn add_user(&self, data: User) -> TreeResult<UserId> {
        self.users.add(data)
    }

    pub(crate) fn sync_users(
        &self,
        new_users: impl IntoIterator<Item = Edge<User>>,
    ) -> TreeResult<()> {
        self.users.insert_edges(new_users)
    }

    pub fn user(&self, id: UserId) -> LensResult<'_, Edge<User>, S> {
        Ok(self.lens(self.users.get_edge(id)?.ok_or(IdNotFound(id))?))
    }
}

impl<'db, S> Lens<'db, Edge<User>, S> {
    #[allow(unused)] // TODO
    pub fn messages(
        &self,
        pagination: Pagination<MessageId>,
    ) -> LensResult<'db, Page<Message, MessageId>, S> {
        self.lens_ref().map_lens(|focus, store| {
            let prefixed_pagination = pagination.add_prefix(focus.cursor);
            store
                .messages_by_author
                .page_mapped(prefixed_pagination, |compound, ()| {
                    Ok(store.message(compound.tail().single())?.focus)
                })
        })
    }
}
