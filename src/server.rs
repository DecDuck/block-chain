use core::ptr::addr_of_mut;

use embassy_net::tcp::TcpSocket;
use log::{info, warn};
use mcproto_rs::{
    Deserialize as _,
    protocol::{Id, Packet, PacketDirection, RawPacket as _, State},
    types::VarInt,
    v1_16_3::{Packet753, RawPacket753, RawPacket753Body},
};

use crate::{encryption::ServerEncryption, packet::{process_packet, PlayerContext, VAR_INT_BUF_SIZE}};

const RX_BUFFER_SIZE: usize = 16384;
const TX_BUFFER_SIZE: usize = 16384;
const READ_BUF: usize = 4096;
const MAX_PACKET_LENGTH: u32 = 1024 * 64;

static mut RX_BUFFER: [u8; RX_BUFFER_SIZE] = [0; RX_BUFFER_SIZE];
static mut TX_BUFFER: [u8; TX_BUFFER_SIZE] = [0; TX_BUFFER_SIZE];

#[embassy_executor::task]
pub async fn start_tcp_server(stack: embassy_net::Stack<'static>, encryption: &'static ServerEncryption<'static>) {
    loop {
        let mut socket = TcpSocket::new(stack, unsafe { &mut *addr_of_mut!(RX_BUFFER) }, unsafe {
            &mut *addr_of_mut!(TX_BUFFER)
        });

        socket.accept(25565).await.expect("failed to accept socket");

        let remote = socket.remote_endpoint();
        info!("recieved connection from {:?}", remote);

        match handle_connection(socket, encryption).await {
            Ok(_) => (),
            Err(err) => warn!("error while handing connection {err:?}"),
        }
    }
}

async fn read_socket(
    socket: &mut TcpSocket<'_>,
    buf: &mut [u8],
    written: &mut usize,
) -> Result<(), embassy_net::tcp::Error> {
    let len = socket.read(&mut buf[*written..]).await?;
    *written += len;
    Ok(())
}

async fn read_packet_length(
    socket: &mut TcpSocket<'_>,
    read_buf: &mut [u8],
    used: &mut usize,
    written_to_amount: &mut usize,
) -> Result<u32, embassy_net::tcp::Error> {
    let mut accumulator = 0u32;
    let mut position = 0;

    loop {
        while position >= (*written_to_amount - *used) {
            read_socket(socket, read_buf, written_to_amount).await?;
        }
        let current_byte = read_buf[position];
        accumulator |= (current_byte as u32 & 0x7f) << (position * 7);
        position += 1;
        *used += 1;
        if (current_byte & 0x80) == 0 {
            break;
        }
        if position >= VAR_INT_BUF_SIZE {
            return Err(embassy_net::tcp::Error::ConnectionReset);
        }
    }

    Ok(accumulator)
}

pub async fn handle_connection<'a>(
    mut socket: TcpSocket<'a>,
    encryption: &'static ServerEncryption<'static>,
) -> Result<(), embassy_net::tcp::Error> {
    let mut read_buf = [0u8; READ_BUF];
    let mut written_to = 0;
    let mut used = 0;

    let mut context = PlayerContext::default();

    loop {
        let packet_length = read_packet_length(
            &mut socket,
            &mut read_buf[used..],
            &mut used,
            &mut written_to,
        )
        .await?;

        info!("reading packet of size: {}", packet_length);

        if packet_length > MAX_PACKET_LENGTH {
            return Err(embassy_net::tcp::Error::ConnectionReset);
        }

        info!("used {} of {}", used, written_to);

        while (written_to - used) < packet_length.try_into().unwrap() {
            read_socket(&mut socket, &mut read_buf, &mut written_to).await?;
        }

        let end = used + packet_length as usize;

        info!(
            "reading from {} to {} for size of {}",
            used, end, packet_length
        );

        let packet_id = VarInt::mc_deserialize(&read_buf[used..end])
            .map_err(|_| embassy_net::tcp::Error::ConnectionReset)?;

        used += packet_length as usize;

        info!("reading server-bound packet of id: {}", packet_id.value);

        let id = Id {
            id: *packet_id.value,
            state: context.state,
            direction: PacketDirection::ServerBound,
        };

        let packet =
            RawPacket753::create(id, packet_id.data).expect("failed to convert to raw packet");
        let packet = packet.deserialize();
        match packet {
            Ok(packet) => {
                let should_continue = process_packet(packet, &mut context, &mut socket).await?;
                if !should_continue {
                    break;
                }
            }
            Err(err) => {
                warn!("failed to read packet: {:?}", err);
            }
        }
    }

    Ok(())
}
