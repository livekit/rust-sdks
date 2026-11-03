// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use livekit::prelude::ParticipantIdentity;

/// One entry of a session's message feed: a transcript segment or a chat
/// message.
///
/// A message is identified by [`id`](Self::id) together with
/// [`participant_identity`](Self::participant_identity). A transcript is
/// updated in place as it grows, keeping its `seq` and `timestamp`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// The transcript segment id, or the text-stream id for chat and user
    /// input.
    pub id: String,
    /// Position in the feed, assigned when the message is first received.
    /// Messages are ordered by this rather than by `timestamp`, which comes
    /// from the sender's clock.
    pub seq: u64,
    /// When the sender started the message, by the sender's clock.
    pub timestamp: DateTime<Utc>,
    /// Where the message came from.
    pub origin: MessageOrigin,
    /// The participant who sent or spoke the message.
    pub participant_identity: ParticipantIdentity,
    /// The text received so far.
    pub text: String,
    /// Whether `text` is complete. An incomplete transcript is updated again.
    pub is_final: bool,
}

/// Where a [`Message`] came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageOrigin {
    /// A transcript of what the agent said.
    AgentTranscript,
    /// A transcript of what the local user said.
    UserTranscript,
    /// Text the local user sent with [`Session::send_text`](crate::Session::send_text).
    UserInput,
    /// Chat sent by another participant.
    Chat,
}

/// A message as a source reports it, before the store assigns its `seq`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MessageUpdate {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub origin: MessageOrigin,
    pub participant_identity: ParticipantIdentity,
    pub text: String,
    pub is_final: bool,
}

/// The message feed: messages deduplicated by id and participant, in receive
/// order.
#[derive(Debug, Default)]
pub(crate) struct MessageStore {
    /// Ordered by `seq`.
    messages: Vec<Message>,
    index: HashMap<(String, ParticipantIdentity), usize>,
    next_seq: u64,
}

impl MessageStore {
    /// Inserts a new message, or updates an existing one in place, keeping its
    /// `seq`.
    pub(crate) fn upsert(&mut self, update: MessageUpdate) -> Message {
        let key = (update.id, update.participant_identity);
        if let Some(&position) = self.index.get(&key) {
            let message = &mut self.messages[position];
            message.timestamp = update.timestamp;
            message.origin = update.origin;
            message.text = update.text;
            message.is_final = update.is_final;
            return message.clone();
        }

        let (id, participant_identity) = key.clone();
        let message = Message {
            id,
            seq: self.next_seq,
            timestamp: update.timestamp,
            origin: update.origin,
            participant_identity,
            text: update.text,
            is_final: update.is_final,
        };
        self.next_seq += 1;
        self.index.insert(key, self.messages.len());
        self.messages.push(message.clone());
        message
    }

    /// All messages, ordered by `seq`.
    pub(crate) fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Replaces the feed with `messages`, numbered from zero in the order
    /// given. Of messages sharing an id and participant, the last wins.
    pub(crate) fn restore(&mut self, messages: &[Message]) {
        *self = Self::default();
        for message in messages {
            self.upsert(MessageUpdate {
                id: message.id.clone(),
                timestamp: message.timestamp,
                origin: message.origin,
                participant_identity: message.participant_identity.clone(),
                text: message.text.clone(),
                is_final: message.is_final,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::TimeZone;

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
    }

    fn update(id: &str, seconds: i64, text: &str) -> MessageUpdate {
        MessageUpdate {
            id: id.into(),
            timestamp: at(seconds),
            origin: MessageOrigin::Chat,
            participant_identity: "remote".to_owned().into(),
            text: text.into(),
            is_final: true,
        }
    }

    #[test]
    fn upsert_keeps_seq_and_orders_by_receive_order() {
        let mut store = MessageStore::default();
        // The second message has the earlier sender clock.
        assert_eq!(store.upsert(update("a", 10, "first")).seq, 0);
        assert_eq!(store.upsert(update("b", 0, "second")).seq, 1);
        let updated = store.upsert(update("a", 10, "first, edited"));
        assert_eq!(updated.seq, 0);

        let feed: Vec<_> = store.messages().iter().map(|m| (m.id.as_str(), m.seq)).collect();
        assert_eq!(feed, [("a", 0), ("b", 1)]);
        assert_eq!(store.messages()[0].text, "first, edited");
    }

    #[test]
    fn restore_keeps_the_given_order_and_renumbers() {
        let mut store = MessageStore::default();
        store.upsert(update("stale", 0, "dropped by restore"));

        // Received first, but stamped later by a skewed sender clock.
        let mut first = store.upsert(update("first", 20, "first"));
        first.seq = 7;
        let mut second = first.clone();
        second.id = "second".into();
        second.timestamp = at(10);
        second.seq = 3;

        store.restore(&[first, second]);
        let feed: Vec<_> = store.messages().iter().map(|m| (m.id.as_str(), m.seq)).collect();
        assert_eq!(feed, [("first", 0), ("second", 1)]);
        assert_eq!(store.upsert(update("next", 0, "next")).seq, 2);
    }
}
