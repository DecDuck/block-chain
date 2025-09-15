use crate::alloc::borrow::ToOwned;
use crate::{types::*, uuid::*, *};
use alloc::fmt;
use alloc::string::String;
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
   },
   LoginSuccess, 0x02, Login, ClientBound => LoginSuccessSpec {
    uuid: UUID4,
    username: String,
    properties: CountedArray<LoginSuccessProperty, VarInt>
   }
});

#[derive(Debug, Clone, PartialEq)]
pub struct LoginSuccessProperty {
    name: String,
    value: String,
    signature: Option<String>,
}

impl Serialize for LoginSuccessProperty {
    fn mc_serialize<S: Serializer>(&self, to: &mut S) -> SerializeResult {
        to.serialize_other(&self.name)?;
        to.serialize_other(&self.value)?;
        if let Some(sign) = &self.signature {
            to.serialize_byte(0x01)?;
            to.serialize_other(sign)?;
        } else {
            to.serialize_byte(0x00)?;
        }
        Ok(())
    }
}

impl Deserialize for LoginSuccessProperty {
    fn mc_deserialize(data: &[u8]) -> DeserializeResult<'_, Self> {
        let name = String::mc_deserialize(data)?;
        let value = String::mc_deserialize(name.data)?;
        let has_signature = bool::mc_deserialize(value.data)?;
        let (signature, leftover) = if has_signature.value {
            let result = String::mc_deserialize(value.data)?;
            (Some(result.value), result.data)
        } else {
            (None, has_signature.data)
        };

        Ok(Deserialized {
            value: Self {
                name: name.value,
                value: value.value,
                signature: signature,
            },
            data: leftover,
        })
    }
}
