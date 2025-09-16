use alloc::{borrow::ToOwned as _, vec::Vec};
use embassy_net::tcp::TcpSocket;
use log::{info, warn};
use mcproto_rs::{protocol::State, status::{StatusPlayersSpec, StatusSpec, StatusVersionSpec}, types::Chat, v1_21_8::{HandshakeIntent, Packet772, PingResponseSpec, StatusResponseSpec}};

use crate::{encryption::ServerEncryption, errors::MinecraftError, packets::{write_packet, PlayerContext}, utils::text};

pub async fn handle_status_packets(
    packet: Packet772,
    context: &mut PlayerContext,
    socket: &mut TcpSocket<'_>,
    encryption: &ServerEncryption<'static>,
) -> Result<(Option<Packet772>, bool), MinecraftError> {
    match packet {
        Packet772::PingRequest(v) => {
            let response = Packet772::PingResponse(PingResponseSpec { payload: v.payload });

            write_packet(socket, context, response).await?;
            return Ok((None, true));
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
            };

            return Ok((None, true));
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

            write_packet(socket, context, response).await?;
            return Ok((None, true));
        }
        _ => Ok((Some(packet), true)),
    }
}
