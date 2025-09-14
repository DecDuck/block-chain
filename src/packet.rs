use alloc::{borrow::ToOwned as _, string::String, vec::Vec};
use embassy_net::tcp::TcpSocket;
use log::{info, warn};
use mcproto_rs::{
    Serialize,
    protocol::{HasPacketBody, HasPacketId, State},
    status::{StatusPlayersSpec, StatusSpec, StatusVersionSpec},
    types::{Chat, CountedArray, VarInt},
    v1_21_8::{
        HandshakeIntent, LoginEncryptionRequestSpec, Packet772, PingResponseSpec,
        StatusResponseSpec,
    },
};
use rsa::pkcs8::der::Encode;

use crate::{
    encryption::ServerEncryption,
    utils::{SliceSerializer, text},
};

const PACKET_WRITE_BUFFER_SIZE: usize = 4096;
pub const VAR_INT_BUF_SIZE: usize = 5;
pub const EMPTY_STRING: String = String::new();

pub struct PlayerContext {
    pub state: State,
    random_key: Option<Vec<u8>>,
}

impl Default for PlayerContext {
    fn default() -> Self {
        Self {
            state: State::Handshaking,
            random_key: None,
        }
    }
}

async fn write_packet(
    socket: &mut TcpSocket<'_>,
    packet: Packet772,
) -> Result<(), embassy_net::tcp::Error> {
    let mut serializer_backend = [0u8; PACKET_WRITE_BUFFER_SIZE];
    let mut serializer = SliceSerializer::create(&mut serializer_backend);
    packet
        .id()
        .mc_serialize(&mut serializer)
        .map_err(|v| embassy_net::tcp::Error::ConnectionReset)?;
    packet
        .mc_serialize_body(&mut serializer)
        .map_err(|v| embassy_net::tcp::Error::ConnectionReset)?;

    let packet = serializer.finish();

    let mut length_serializer_backend = [0u8; VAR_INT_BUF_SIZE];
    let mut length_serializer = SliceSerializer::create(&mut length_serializer_backend);
    let packet_len = VarInt(packet.len().try_into().unwrap());
    packet_len
        .mc_serialize(&mut length_serializer)
        .expect("failed to serialize packet length");
    let packet_len_len = length_serializer.finish().len();

    let mut written = 0;
    while written < packet_len_len {
        written += socket
            .write(&length_serializer_backend[written..packet_len_len])
            .await?;
    }
    written = 0;

    while written < packet.len() {
        written += socket.write(&packet[written..]).await?;
    }

    Ok(())
}

pub async fn process_packet(
    packet: Packet772,
    context: &mut PlayerContext,
    socket: &mut TcpSocket<'_>,
    encryption: &ServerEncryption<'static>,
) -> Result<bool, embassy_net::tcp::Error> {
    // info!("{:?}", packet);
    match packet {
        Packet772::PingRequest(v) => {
            let response = Packet772::PingResponse(PingResponseSpec { payload: v.payload });

            write_packet(socket, response).await?;
            return Ok(true);
        }
        Packet772::Handshake(v) => {
            info!("trying to connect with version: {}", v.protocol_version);

            match v.intent {
                HandshakeIntent::Status => {
                    context.state = State::Status;
                }
                HandshakeIntent::Login => {
                    context.state = State::Login;
                }
                HandshakeIntent::Transfer => {
                    warn!("transfer not supported")
                }
            }
        }
        Packet772::StatusRequest(_) => {
            let response = Packet772::StatusResponse(StatusResponseSpec {
                response: StatusSpec {
                    version: Some(StatusVersionSpec {
                        name: "1.21.8".to_owned(),
                        protocol: 772,
                    }),
                    players: StatusPlayersSpec {
                        max: 10,
                        online: 0,
                        sample: Vec::new(),
                    },
                    description: Chat::Text(text(
                        "blockchain - instead of bitcoin, a block game for your key chain!",
                    )),
                    favicon: None,
                    enforces_secure_chat: false,
                },
            });

            write_packet(socket, response).await?;
            return Ok(true);
        }
        Packet772::LoginStart(spec) => {
            info!("{} is connecting...", spec.name);

            let spki = rsa::pkcs8::SubjectPublicKeyInfo::from_key(&encryption.public)
                .expect("failed to create spki")
                .to_der()
                .expect("failed to serialize to der");

            let random = encryption.random_data().await;

            let encryption_request =
                Packet772::LoginEncryptionRequest(LoginEncryptionRequestSpec {
                    server_id: EMPTY_STRING,
                    public_key: CountedArray::from(spki),
                    verify_token: CountedArray::from(random),
                    should_authenticate: false,
                });

            write_packet(socket, encryption_request).await?;
            return Ok(true);
        }
        _ => {
            info!("no handler for type {:?}", packet.id());
        }
    };

    Ok(true)
}
