use crate::{types::*, uuid::*, *};
use alloc::fmt;
use alloc::{
    borrow::ToOwned,
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use fmt::Debug;

proto_byte_enum!(HandshakeIntent,
    0x01 :: Status,
    0x02 :: Login,
    0x03 :: Transfer
);

define_protocol!(772, Packet772, RawPacket772, RawPacket772Body, Packet772Kind => {
    PingRequest, 0x01, Status, ServerBound => PingRequestSpec {
        payload: u64
    },
    PingResponse, 0x01, Status, ClientBound => PingResponseSpec {
        payload: u64
    },
   Handshake, 0x00, Handshaking, ServerBound => HandshakeSpec {
    protocol_version: VarInt,
    server_address: String,
    server_port: u16,
    intent: HandshakeIntent
   },
   StatusRequest, 0x00, Status, ServerBound => StatusRequestSpec {
   },
   StatusResponse, 0x00, Status, ClientBound => StatusResponseSpec {
    response: super::status::StatusSpec
   },
   LoginStart, 0x00, Login, ServerBound => LoginStartSpec {
    name: String,
    uuid: UUID4
   },
   LoginEncryptionRequest, 0x01, Login, ClientBound => LoginEncryptionRequestSpec {
    server_id: String,
    public_key: CountedArray<u8, VarInt>,
    verify_token: CountedArray<u8, VarInt>,
    should_authenticate: bool
   },
   LoginEncryptionResponse, 0x01, Login, ServerBound => LoginEncryptionResponseSpec {
    shared_secret: CountedArray<u8, VarInt>,
    verify_token: CountedArray<u8, VarInt>
   }
});
