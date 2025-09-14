use alloc::{borrow::ToOwned as _, string::String, vec::Vec};
use embassy_net::tcp::TcpSocket;
use log::info;
use mcproto_rs::{
    Serialize,
    protocol::{HasPacketBody, HasPacketId, State},
    status::{StatusPlayersSpec, StatusSpec, StatusVersionSpec},
    types::{BytesSerializer, Chat, TextComponent, VarInt},
    v1_16_3::{
        HandshakeNextState, LoginDisconnectSpec, LoginEncryptionRequestSpec, Packet753,
        StatusResponseSpec,
    },
};

use crate::utils::{SliceSerializer, text};

const PACKET_WRITE_BUFFER_SIZE: usize = 4096;
pub const VAR_INT_BUF_SIZE: usize = 5;
pub const EMPTY_STRING: String = String::new();

pub struct PlayerContext {
    pub state: State,
}

impl Default for PlayerContext {
    fn default() -> Self {
        Self {
            state: State::Handshaking,
        }
    }
}

async fn write_packet(
    socket: &mut TcpSocket<'_>,
    packet: Packet753,
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
    packet: Packet753,
    context: &mut PlayerContext,
    socket: &mut TcpSocket<'_>,
) -> Result<bool, embassy_net::tcp::Error> {
    info!("{:?}", packet);
    match packet {
        Packet753::Handshake(v) => {
            info!("trying to connect with version: {}", v.version);

            match v.next_state {
                HandshakeNextState::Status => {
                    context.state = State::Status;
                }
                HandshakeNextState::Login => {
                    context.state = State::Login;
                }
            }
        }
        Packet753::StatusRequest(spec) => {
            let response = Packet753::StatusResponse(StatusResponseSpec {
                response: StatusSpec {
                    version: Some(StatusVersionSpec {
                        name: "1.16.3".to_owned(),
                        protocol: 753,
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
                },
            });

            write_packet(socket, response).await?;
            return Ok(true);
        }
        Packet753::LoginStart(spec) => {
            info!("{} is connecting...", spec.name);

            let encryption_request =
                Packet753::LoginEncryptionRequest(LoginEncryptionRequestSpec {
                    server_id: EMPTY_STRING,
                    public_key: todo!(),
                    verify_token: todo!(),
                });
        }
        _ => (),
    };

    Ok(true)
}
