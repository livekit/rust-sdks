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

use crate::constants::{ATTRIBUTE_SEGMENT_ID, ATTRIBUTE_TRANSCRIPTION_FINAL};

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

/// An incoming `lk.transcription` stream. Its attributes are passed
/// separately on every call, since a trailer can still change them.
#[derive(Clone, Debug)]
pub(crate) struct TranscriptStream {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub participant_identity: ParticipantIdentity,
    /// Whether the transcript is of the local user, i.e. the agent sent it on
    /// the local participant's behalf.
    pub is_local: bool,
}

impl TranscriptStream {
    /// The segment id, falling back to the stream id when the agent sent
    /// none.
    fn segment_id(&self, attributes: &HashMap<String, String>) -> String {
        attributes.get(ATTRIBUTE_SEGMENT_ID).cloned().unwrap_or_else(|| self.id.clone())
    }

    fn origin(&self) -> MessageOrigin {
        if self.is_local {
            MessageOrigin::UserTranscript
        } else {
            MessageOrigin::AgentTranscript
        }
    }
}

/// Parses `lk.transcription_final`. Agents write the strings `"true"` and
/// `"false"`; anything else counts as absent.
fn parse_final(attributes: &HashMap<String, String>) -> Option<bool> {
    let value = attributes.get(ATTRIBUTE_TRANSCRIPTION_FINAL)?.trim();
    if value.eq_ignore_ascii_case("true") {
        Some(true)
    } else if value.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

/// A transcript segment still being received.
#[derive(Debug)]
struct Partial {
    text: String,
    /// The timestamp of the segment's first stream, kept as the segment grows.
    timestamp: DateTime<Utc>,
    /// The stream that last wrote the segment.
    stream_id: String,
}

/// Turns `lk.transcription` chunks into transcript messages.
///
/// Agents send transcripts in two shapes on the same topic. Agent speech is
/// one stream whose chunks append to the text, finalized by the stream's
/// trailer. User speech is a new stream per interim result, each carrying the
/// full text so far, with the last one marked final. Both are handled by one
/// rule: a chunk appends when it arrives on the stream that last wrote the
/// segment, and replaces the text when it arrives on a new stream.
#[derive(Debug, Default)]
pub(crate) struct TranscriptAggregator {
    /// Keyed by segment and participant: the same segment id from two
    /// participants is two messages.
    partials: HashMap<(String, ParticipantIdentity), Partial>,
}

impl TranscriptAggregator {
    /// Applies a chunk, returning the updated message. Empty chunks change
    /// nothing.
    pub(crate) fn on_chunk(
        &mut self,
        stream: &TranscriptStream,
        attributes: &HashMap<String, String>,
        chunk: &str,
    ) -> Option<MessageUpdate> {
        if chunk.is_empty() {
            return None;
        }
        let segment_id = stream.segment_id(attributes);
        let key = (segment_id.clone(), stream.participant_identity.clone());

        if !self.partials.contains_key(&key) {
            // A new segment means the participant's previous turn is over.
            self.partials.retain(|(segment, participant), _| {
                participant != &stream.participant_identity || segment == &segment_id
            });
        }
        let partial = self.partials.entry(key).or_insert_with(|| Partial {
            text: String::new(),
            timestamp: stream.timestamp,
            stream_id: stream.id.clone(),
        });
        if partial.stream_id == stream.id {
            partial.text.push_str(chunk);
        } else {
            partial.text = chunk.to_owned();
            partial.stream_id = stream.id.clone();
        }

        Some(MessageUpdate {
            id: segment_id,
            timestamp: partial.timestamp,
            origin: stream.origin(),
            participant_identity: stream.participant_identity.clone(),
            text: partial.text.clone(),
            is_final: false,
        })
    }

    /// Handles the end of a stream, returning the finalized message if the
    /// stream completed its segment.
    ///
    /// A stream completes its segment unless it is explicitly marked not
    /// final (an interim user transcript) or a newer stream has taken the
    /// segment over.
    pub(crate) fn on_stream_end(
        &mut self,
        stream: &TranscriptStream,
        attributes: &HashMap<String, String>,
    ) -> Option<MessageUpdate> {
        if parse_final(attributes) == Some(false) {
            return None;
        }
        let segment_id = stream.segment_id(attributes);
        let key = (segment_id, stream.participant_identity.clone());
        if self.partials.get(&key)?.stream_id != stream.id {
            return None;
        }
        let ((id, participant_identity), partial) = self.partials.remove_entry(&key)?;

        Some(MessageUpdate {
            id,
            timestamp: partial.timestamp,
            origin: stream.origin(),
            participant_identity,
            text: partial.text,
            is_final: true,
        })
    }
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

    /// Replaces the feed with `messages`, ordered by `timestamp` and numbered
    /// from zero. Of messages sharing an id and participant, the last wins.
    pub(crate) fn restore(&mut self, messages: &[Message]) {
        let mut restored = messages.to_vec();
        restored.sort_by_key(|message| message.timestamp);

        *self = Self::default();
        for message in restored {
            self.upsert(MessageUpdate {
                id: message.id,
                timestamp: message.timestamp,
                origin: message.origin,
                participant_identity: message.participant_identity,
                text: message.text,
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

    fn stream(id: &str, participant: &str, seconds: i64) -> TranscriptStream {
        TranscriptStream {
            id: id.into(),
            timestamp: at(seconds),
            participant_identity: participant.to_owned().into(),
            is_local: participant == "user",
        }
    }

    fn attributes(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn segment(id: &str, is_final: &str) -> HashMap<String, String> {
        attributes(&[(ATTRIBUTE_SEGMENT_ID, id), (ATTRIBUTE_TRANSCRIPTION_FINAL, is_final)])
    }

    fn text(update: Option<MessageUpdate>) -> Option<(String, bool)> {
        update.map(|update| (update.text, update.is_final))
    }

    #[test]
    fn chunks_on_one_stream_append() {
        let mut aggregator = TranscriptAggregator::default();
        let agent = stream("s1", "agent", 0);
        let open = segment("seg", "false");

        aggregator.on_chunk(&agent, &open, "Hello");
        aggregator.on_chunk(&agent, &open, " world");
        let update = aggregator.on_chunk(&agent, &open, "!").unwrap();
        assert_eq!(update.text, "Hello world!");
        assert_eq!(update.origin, MessageOrigin::AgentTranscript);
        assert!(!update.is_final);

        // The trailer marks the segment final before the stream ends.
        let update = aggregator.on_stream_end(&agent, &segment("seg", "true")).unwrap();
        assert_eq!((update.id.as_str(), update.text.as_str()), ("seg", "Hello world!"));
        assert!(update.is_final);
        assert!(aggregator.partials.is_empty());
    }

    #[test]
    fn a_new_stream_for_the_same_segment_replaces() {
        let mut aggregator = TranscriptAggregator::default();
        let interim = segment("seg", "false");

        let first = stream("s1", "user", 0);
        aggregator.on_chunk(&first, &interim, "Hello");
        assert_eq!(aggregator.on_stream_end(&first, &interim), None);

        let second = stream("s2", "user", 5);
        let update = aggregator.on_chunk(&second, &interim, "Hello world").unwrap();
        assert_eq!(update.text, "Hello world");
        assert_eq!(update.origin, MessageOrigin::UserTranscript);
        assert_eq!(update.timestamp, at(0), "timestamp stays at the segment's first stream");
        aggregator.on_stream_end(&second, &interim);

        let last = stream("s3", "user", 9);
        let done = segment("seg", "true");
        aggregator.on_chunk(&last, &done, "Hello world!");
        let update = aggregator.on_stream_end(&last, &done).unwrap();
        assert_eq!((update.text.as_str(), update.is_final), ("Hello world!", true));
        assert_eq!(update.timestamp, at(0));
    }

    #[test]
    fn id_falls_back_to_the_stream_id() {
        let mut aggregator = TranscriptAggregator::default();
        let update = aggregator.on_chunk(&stream("s1", "agent", 0), &HashMap::new(), "Hi").unwrap();
        assert_eq!(update.id, "s1");
    }

    #[test]
    fn final_attribute_parsing() {
        for (value, parsed) in [
            ("true", Some(true)),
            ("TRUE", Some(true)),
            (" true ", Some(true)),
            ("false", Some(false)),
            ("yes", None),
        ] {
            let attrs = attributes(&[(ATTRIBUTE_TRANSCRIPTION_FINAL, value)]);
            assert_eq!(parse_final(&attrs), parsed, "{value:?}");
        }
        assert_eq!(parse_final(&HashMap::new()), None);
    }

    #[test]
    fn stream_ending_without_final_attribute_finalizes() {
        let mut aggregator = TranscriptAggregator::default();
        let agent = stream("s1", "agent", 0);
        let attrs = attributes(&[(ATTRIBUTE_SEGMENT_ID, "seg")]);
        aggregator.on_chunk(&agent, &attrs, "Hi");
        assert_eq!(text(aggregator.on_stream_end(&agent, &attrs)), Some(("Hi".into(), true)));
    }

    #[test]
    fn stream_ending_after_being_superseded_does_not_finalize() {
        let mut aggregator = TranscriptAggregator::default();
        let attrs = attributes(&[(ATTRIBUTE_SEGMENT_ID, "seg")]);
        let first = stream("s1", "user", 0);
        let second = stream("s2", "user", 1);
        aggregator.on_chunk(&first, &attrs, "Hel");
        aggregator.on_chunk(&second, &attrs, "Hello");
        assert_eq!(aggregator.on_stream_end(&first, &attrs), None);
        assert_eq!(text(aggregator.on_stream_end(&second, &attrs)), Some(("Hello".into(), true)));
    }

    #[test]
    fn empty_chunks_are_skipped() {
        let mut aggregator = TranscriptAggregator::default();
        assert_eq!(aggregator.on_chunk(&stream("s1", "agent", 0), &HashMap::new(), ""), None);
        assert!(aggregator.partials.is_empty());
    }

    #[test]
    fn same_segment_from_two_participants_is_two_messages() {
        let mut aggregator = TranscriptAggregator::default();
        let mut store = MessageStore::default();
        let attrs = segment("seg", "false");
        store.upsert(aggregator.on_chunk(&stream("s1", "agent", 0), &attrs, "A").unwrap());
        store.upsert(aggregator.on_chunk(&stream("s2", "user", 0), &attrs, "B").unwrap());
        assert_eq!(aggregator.partials.len(), 2);
        assert_eq!(store.messages().len(), 2);
    }

    #[test]
    fn a_new_segment_drops_the_participants_previous_partials() {
        let mut aggregator = TranscriptAggregator::default();
        aggregator.on_chunk(&stream("s1", "agent", 0), &segment("one", "false"), "first");
        aggregator.on_chunk(&stream("s2", "user", 0), &segment("one", "false"), "mine");
        aggregator.on_chunk(&stream("s3", "agent", 1), &segment("two", "false"), "second");

        let mut keys: Vec<_> = aggregator
            .partials
            .keys()
            .map(|(segment, participant)| (segment.as_str(), participant.as_str()))
            .collect();
        keys.sort();
        assert_eq!(keys, [("one", "user"), ("two", "agent")]);
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
    fn restore_sorts_by_timestamp_and_renumbers() {
        let mut store = MessageStore::default();
        store.upsert(update("stale", 0, "dropped by restore"));

        let mut later = store.upsert(update("later", 20, "later"));
        later.seq = 7;
        let mut earlier = later.clone();
        earlier.id = "earlier".into();
        earlier.timestamp = at(10);
        earlier.seq = 3;

        store.restore(&[later, earlier]);
        let feed: Vec<_> = store.messages().iter().map(|m| (m.id.as_str(), m.seq)).collect();
        assert_eq!(feed, [("earlier", 0), ("later", 1)]);
        assert_eq!(store.upsert(update("next", 0, "next")).seq, 2);
    }
}
