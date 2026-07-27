import asyncio
import livekit_uniffi

class OutgoingDelegate(livekit_uniffi.OutgoingDataStreamManagerDelegate):
    def on_packets_available(self, packets):
        print('PACKETS:', packets)

class RemoteParticipantRegistry(livekit_uniffi.RemoteParticipantRegistryDelegate):
    def remote_capabilities(self, identity):
        return [] # typing.List[ClientCapability]

    def remote_client_protocol(self, identity):
        return 2

    def remote_identities(self):
        return ["alice", "bob", "randy"]

class IncomingDelegate(livekit_uniffi.IncomingDataStreamManagerDelegate):
    """Forwards opened readers onto the main asyncio loop.

    Delegate callbacks fire on a Rust tokio thread, so they must not block or await;
    hand the reader off to the main loop and let it drive the async reads.
    """

    def __init__(self, loop: asyncio.AbstractEventLoop, opened: asyncio.Queue):
        self._loop = loop
        self._opened = opened

    def on_byte_stream_opened(self, reader, identity: str):
        self._loop.call_soon_threadsafe(self._opened.put_nowait, ("byte", reader, identity))

    def on_text_stream_opened(self, reader, identity: str):
        self._loop.call_soon_threadsafe(self._opened.put_nowait, ("text", reader, identity))

# Encoded livekit.DataPacket envelopes (participant_identity = "alice") carrying a
# DataStream.Header / Chunk / Trailer for an 11-byte "hello world" text stream.
DATA_STREAM_HEADER_BYTES = b'"\x05alicej@\n\x11example-stream-id\x10\xad\xf5\xcb\xae\xf93\x1a\x08my-topic"\ntext/plain(\x0bB\n\n\x03foo\x12\x03barJ\x00'
DATA_STREAM_CHUNK_BYTES = b'"\x05alicer \n\x11example-stream-id\x1a\x0bhello world'
DATA_STREAM_TRAILER_BYTES = b'"\x05alicez\'\n\x11example-stream-id\x1a\x12\n\x06status\x12\x08complete'

async def main():
    opened = asyncio.Queue()

    print("--- OUTGOING:")
    outgoing_delegate = OutgoingDelegate()
    remote_participant_registry = RemoteParticipantRegistry()
    outgoing = livekit_uniffi.OutgoingDataStreamManager(outgoing_delegate, remote_participant_registry)
    await outgoing.send_text('hello world', livekit_uniffi.StreamTextOptions(
        topic="test",
        attributes={},
        # destination_identities: 'typing.List[str]' = <object object at 0x10089cc40>,
        # id: 'typing.Optional[str]' = <object object at 0x10089cc40>,
        # operation_type: 'typing.Optional[OperationType]' = <object object at 0x10089cc40>,
        # version: 'typing.Optional[int]' = <object object at 0x10089cc40>,
        # reply_to_stream_id: 'typing.Optional[str]' = <object object at 0x10089cc40>,
        # attached_stream_ids: 'typing.List[str]' = <object object at 0x10089cc40>,
        # generated: 'typing.Optional[bool]' = <object object at 0x10089cc40>,
        # compress: 'typing.Optional[bool]' = <object object at 0x10089cc40>,
        # sender_identity: 'typing.Optional[str]' = <object object at 0x10089cc40>
    ))

    print("--- INCOMING:")
    incoming_delegate = IncomingDelegate(asyncio.get_running_loop(), opened)
    incoming = livekit_uniffi.IncomingDataStreamManager(incoming_delegate, [], None)
    incoming.handle_packet_received(DATA_STREAM_HEADER_BYTES)
    incoming.handle_packet_received(DATA_STREAM_CHUNK_BYTES)
    incoming.handle_packet_received(DATA_STREAM_TRAILER_BYTES)

    kind, reader, identity = await asyncio.wait_for(opened.get(), timeout=5)
    print(f"{kind.upper()} STREAM OPENED:", identity, "CONTENTS:", await reader.read_all())

if __name__ == '__main__':
    asyncio.run(main())
