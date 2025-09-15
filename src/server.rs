use core::ptr::addr_of_mut;

use embassy_net::tcp::TcpSocket;
use log::{info, warn};
use mcproto_rs::{
    Deserialize as _,
    protocol::{Id, PacketDirection, RawPacket as _},
    types::VarInt,
    v1_21_8::RawPacket772,
};

use crate::{
    encryption::ServerEncryption, errors::MinecraftError, packet::{process_packet, PlayerContext, VAR_INT_BUF_SIZE}
};

const RX_BUFFER_SIZE: usize = 16384;
const TX_BUFFER_SIZE: usize = 16384;
const READ_BUF: usize = 8128;
const READ_BUF_MAX: usize = READ_BUF * 2;
const MAX_PACKET_LENGTH: u32 = 1024 * 64;

static mut RX_BUFFER: [u8; RX_BUFFER_SIZE] = [0; RX_BUFFER_SIZE];
static mut TX_BUFFER: [u8; TX_BUFFER_SIZE] = [0; TX_BUFFER_SIZE];

#[embassy_executor::task]
pub async fn start_tcp_server(
    stack: embassy_net::Stack<'static>,
    encryption: &'static ServerEncryption<'static>,
) {
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
    if len == 0 {
        return Err(embassy_net::tcp::Error::ConnectionReset);
    }
    Ok(())
}

async fn read_packet_length(
    socket: &mut TcpSocket<'_>,
    read_buf: &mut [u8],
    read_pointer: &mut usize,
    write_pointer: &mut usize,
) -> Result<u32, MinecraftError> {
    let mut accumulator = 0u32;
    let mut position = 0;

    loop {
        while *read_pointer >= *write_pointer {
            read_socket(socket, read_buf, write_pointer).await?;
        }
        let current_byte = read_buf[*read_pointer];
        accumulator |= (current_byte as u32 & 0x7f) << (position * 7);
        position += 1;
        *read_pointer += 1;
        if (current_byte & 0x80) == 0 {
            break;
        }
        if position >= VAR_INT_BUF_SIZE {
            return Err(MinecraftError::InvalidPacketHeader);
        }
    }

    Ok(accumulator)
}

pub async fn handle_connection<'a>(
    mut socket: TcpSocket<'a>,
    encryption: &'static ServerEncryption<'static>,
) -> Result<(), MinecraftError> {
    let mut read_buf = [0u8; READ_BUF_MAX];
    let mut write_pointer = 0;
    let mut read_pointer = 0;

    let mut context = PlayerContext::default();

    loop {
        if read_pointer >= READ_BUF {
            read_buf.copy_within(read_pointer..write_pointer, 0);
            write_pointer -= read_pointer;
            read_pointer = 0;
            info!("reset ring buffer");
        }

        let packet_length = read_packet_length(
            &mut socket,
            &mut read_buf,
            &mut read_pointer,
            &mut write_pointer,
        )
        .await?;

        info!("reading packet of size: {}", packet_length);

        if packet_length > MAX_PACKET_LENGTH {
            return Err(MinecraftError::InvalidPacketHeader);
        }
        if packet_length == 0 {
            continue;
        }

        while (write_pointer - read_pointer) < packet_length.try_into().unwrap() {
            read_socket(&mut socket, &mut read_buf, &mut write_pointer).await?;
        }

        let end = read_pointer + packet_length as usize;

        info!(
            "reading from {} to {} for size of {}",
            read_pointer, end, packet_length
        );

        let packet_id = VarInt::mc_deserialize(&read_buf[read_pointer..end])?;

        read_pointer += packet_length as usize;

        let id = Id {
            id: *packet_id.value,
            state: context.state,
            direction: PacketDirection::ServerBound,
        };

        let packet =
            RawPacket772::create(id, packet_id.data).expect("failed to convert to raw packet");
        let packet = packet.deserialize();
        match packet {
            Ok(packet) => {
                let should_continue =
                    process_packet(packet, &mut context, &mut socket, &encryption).await?;
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
