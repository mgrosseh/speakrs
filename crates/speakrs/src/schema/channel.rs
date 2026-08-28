use speakrs_storage::{
    key::UuidKey,
    pagination::{Edge, Page, Pagination},
    tree::TreeResult,
};

use crate::schema::{DataStore, IdNotFound, LensResult};

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum ChannelKind {
    Text,
    Voice,
}

pub type ChannelId = UuidKey<Channel>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Channel {
    #[serde(alias = "channel_type")]
    kind: ChannelKind,
    #[serde(alias = "display_name")]
    name: String,
    desc: String,
}

impl Channel {
    /// Create a text channel
    pub fn text(name: String, desc: String) -> Self {
        Self {
            kind: ChannelKind::Text,
            name,
            desc,
        }
    }
    /// Create a voice channel
    #[allow(unused)] // TODO
    pub fn voice(name: String, desc: String) -> Self {
        Self {
            kind: ChannelKind::Voice,
            name,
            desc,
        }
    }
    pub fn get_name(&self) -> &str {
        self.name.as_str()
    }
    pub fn get_description(&self) -> &str {
        self.desc.as_str()
    }
}

impl<S> DataStore<S> {
    pub fn add_channel(&self, data: Channel) -> TreeResult<ChannelId> {
        self.channels.add(data)
    }
    pub fn channel(&self, id: ChannelId) -> LensResult<'_, Edge<Channel>, S> {
        Ok(self.lens(self.channels.get_edge(id)?.ok_or(IdNotFound(id))?))
    }

    pub(crate) fn sync_channels(
        &self,
        new_channels: impl IntoIterator<Item = Edge<Channel>>,
    ) -> TreeResult<()> {
        self.channels.insert_edges(new_channels)
    }

    pub fn channels(
        &self,
        pagination: Pagination<ChannelId>,
    ) -> LensResult<'_, Page<Channel, ChannelId>, S> {
        Ok(self.lens(self.channels.page(pagination)?))
    }
}
