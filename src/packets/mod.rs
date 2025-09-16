use core::mem;

use aes::cipher::AsyncStreamCipher;
use aes::cipher::BlockEncryptMut;
use aes::cipher::KeyIvInit;
use alloc::{borrow::ToOwned as _, string::String, vec::Vec};
use embassy_net::tcp::TcpSocket;
use log::{info, warn};
use mcproto_rs::{
    Serialize,
    protocol::{HasPacketBody, HasPacketId, State},
    status::{StatusPlayersSpec, StatusSpec, StatusVersionSpec},
    types::{Chat, CountedArray, VarInt},
    uuid::UUID4,
    v1_21_8::{
        HandshakeIntent, LoginEncryptionRequestSpec, LoginSuccessSpec, Packet772, PingResponseSpec,
        StatusResponseSpec,
    },
};
use rsa::pkcs8::der::Encode;

use crate::packets::configuration::handle_configuration_packet;
use crate::packets::login::handle_login_packets;
use crate::packets::status::handle_status_packets;
use crate::{
    encryption::ServerEncryption,
    errors::MinecraftError,
    utils::{SliceSerializer, text},
};

const PACKET_WRITE_BUFFER_SIZE: usize = 4096;
pub const VAR_INT_BUF_SIZE: usize = 5;
pub const EMPTY_STRING: String = String::new();

mod configuration;
mod login;
mod status;

struct PlayerLoginContext {
    verify_token: Option<Vec<u8>>,
    uuid: UUID4,
    username: String,
}

type Aes128Cfb8Enc = cfb8::Encryptor<aes::Aes128>;
type Aes128Cfb8Dec = cfb8::Decryptor<aes::Aes128>;
pub struct PlayerEncryptionContext {
    pub encrypter: Aes128Cfb8Enc,
    pub decrypter: Aes128Cfb8Dec,
    shared_token: [u8; 16],
}

impl PlayerEncryptionContext {
    pub fn new(shared_token: Vec<u8>) -> Self {
        info!("token length {}", shared_token.len() * 8);
        let shared_token: [u8; 16] = shared_token
            .try_into()
            .expect("shared token not long enough");
        Self {
            encrypter: Aes128Cfb8Enc::new(&shared_token.into(), &shared_token.into()),
            decrypter: Aes128Cfb8Dec::new(&shared_token.into(), &shared_token.into()),
            shared_token,
        }
    }
}

pub struct PlayerContext {
    pub state: State,
    login_context: Option<PlayerLoginContext>,
    pub encryption_context: Option<PlayerEncryptionContext>,
}

impl Default for PlayerContext {
    fn default() -> Self {
        Self {
            state: State::Handshaking,
            login_context: None,
            encryption_context: None,
        }
    }
}

async fn write_encryption_transparent<const T: usize>(
    socket: &mut TcpSocket<'_>,
    context: &mut PlayerContext,
    slices: [&mut [u8]; T],
) -> Result<(), MinecraftError> {
    for slice in slices {
        if let Some(encryption) = &mut context.encryption_context {
            for chunk in slice.chunks_mut(1) {
                encryption.encrypter.encrypt_block_mut(chunk.into());
            }
        }

        let mut written = 0;

        while written < slice.len() {
            written += socket.write(&slice[written..]).await?;
        }
    }

    Ok(())
}

async fn write_packet(
    socket: &mut TcpSocket<'_>,
    context: &mut PlayerContext,
    packet: Packet772,
) -> Result<(), MinecraftError> {
    let mut serializer_backend = [0u8; PACKET_WRITE_BUFFER_SIZE];
    let mut serializer = SliceSerializer::create(&mut serializer_backend);
    packet.id().mc_serialize(&mut serializer)?;
    packet.mc_serialize_body(&mut serializer)?;

    let packet = serializer.finish();

    let mut length_serializer_backend = [0u8; VAR_INT_BUF_SIZE];
    let mut length_serializer = SliceSerializer::create(&mut length_serializer_backend);
    let packet_len = VarInt(packet.len().try_into().unwrap());
    packet_len
        .mc_serialize(&mut length_serializer)
        .expect("failed to serialize packet length");
    let packet_len_len = length_serializer.finish().len();

    write_encryption_transparent(
        socket,
        context,
        [&mut length_serializer_backend[..packet_len_len], packet],
    )
    .await?;

    Ok(())
}

pub async fn process_packet(
    packet: Packet772,
    context: &mut PlayerContext,
    socket: &mut TcpSocket<'_>,
    encryption: &ServerEncryption<'static>,
) -> Result<bool, MinecraftError> {
    let leftover = match context.state {
        State::Handshaking | State::Status => {
            let (packet, status) =
                handle_status_packets(packet, context, socket, encryption).await?;
            let packet = if let Some(packet) = packet {
                packet
            } else {
                return Ok(status);
            };
            packet
        }
        State::Login => {
            let (packet, status) =
                handle_login_packets(packet, context, socket, encryption).await?;
            let packet = if let Some(packet) = packet {
                packet
            } else {
                return Ok(status);
            };
            packet
        }
        State::Configuration => {
            let (packet, status) =
                handle_configuration_packet(packet, context, socket, encryption).await?;
            let packet = if let Some(packet) = packet {
                packet
            } else {
                return Ok(status);
            };
            packet
        }
        _ => packet,
    };

    info!("no handler for type {:?}", leftover.id());

    Ok(true)
}
