use alloc::string::String;
use embassy_net::tcp::TcpSocket;
use log::info;
use mcproto_rs::{protocol::State, v1_21_8::{ConfigurationFinishSpec, Packet772}};

use crate::{encryption::ServerEncryption, errors::MinecraftError, packets::{write_packet, PlayerContext}};

pub async fn handle_configuration_packet(
    packet: Packet772,
    context: &mut PlayerContext,
    socket: &mut TcpSocket<'_>,
    encryption: &ServerEncryption<'static>,
) -> Result<(Option<Packet772>, bool), MinecraftError> {
    match packet {
        Packet772::ServerBoundPluginMessage(spec) => {
            match spec.id.as_str() {
                "minecraft:brand" => {
                    let brand = String::from_utf8(spec.data.to_vec()).unwrap();
                    info!("OMG BRAND: {}", brand);
                },
                _ => (),
            }

            return Ok((None, true));
        },
        Packet772::ConfigurationClientInformation(spec) => {
            let response = Packet772::ConfigurationFinish(ConfigurationFinishSpec {});
            write_packet(socket, context, response).await?;

            return Ok((None, true));
        },
        Packet772::ConfigurationFinishAck(_) => {
            context.state = State::Play;

            return Ok((None, true));
        },
        _ => Ok((Some(packet), true)),
    }
}
