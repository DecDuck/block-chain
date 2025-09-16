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

proto_byte_enum!(ChatMode,
    0x00 :: Enabled,
    0x01 :: CommandsOnly,
    0x02 :: Hidden
);

proto_byte_enum!(MainHand,
    0x00 :: Left,
    0x01 :: Right
);

proto_byte_enum!(ParticleStatus,
    0x00 :: All,
    0x01 :: Decreased,
    0x02 :: Minimal
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
    },
    LoginAcknowledged, 0x03, Login, ServerBound => LoginAcknowledgedSpec {
    },
    ConfigurationClientInformation, 0x00, Configuration, ServerBound => ConfigurationClientInformationSpec {
        locale: String,
        view_distance: u8,
        chat_mode: ChatMode,
        chat_colours: bool,
        display_skin_parts: u8,
        main_hand: MainHand,
        text_filtering: bool,
        allow_list_players: bool,
        particle_status: ParticleStatus
    },
    ServerBoundPluginMessage, 0x02, Configuration, ServerBound => ServerBoundPluginMessageSpec {
        id: String,
        data: RemainingBytes
    },
    ConfigurationFinish, 0x03, Configuration, ClientBound => ConfigurationFinishSpec {
    },
    ConfigurationFinishAck, 0x03, Configuration, ServerBound => ConfigurationFinishAckSpec {
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
        to.serialize_other(&self.signature)?;
        Ok(())
    }
}

impl Deserialize for LoginSuccessProperty {
    fn mc_deserialize(data: &[u8]) -> DeserializeResult<'_, Self> {
        let name = String::mc_deserialize(data)?;
        let value = String::mc_deserialize(name.data)?;
        let signature = Option::<String>::mc_deserialize(value.data)?;

        Ok(Deserialized {
            value: Self {
                name: name.value,
                value: value.value,
                signature: signature.value,
            },
            data: signature.data,
        })
    }
}
