#![allow(non_snake_case)]
#![allow(clippy::enum_variant_names)]

use livekit::ChatMessage as RoomChatMessage;

include!("livekit.proto.rs");

impl From<ChatMessage> for RoomChatMessage {
    fn from(proto_msg: ChatMessage) -> Self {
        RoomChatMessage {
            id: proto_msg.id,
            message: proto_msg.message,
            timestamp: proto_msg.timestamp,
            edit_timestamp: proto_msg.edit_timestamp,
            deleted: proto_msg.deleted,
            generated: proto_msg.generated,
        }
    }
}

impl From<RoomChatMessage> for ChatMessage {
    fn from(msg: RoomChatMessage) -> Self {
        ChatMessage {
            id: msg.id,
            message: msg.message,
            timestamp: msg.timestamp,
            edit_timestamp: msg.edit_timestamp,
            deleted: msg.deleted.into(),
            generated: msg.generated.into(),
        }
    }
}
