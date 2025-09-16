use core::{net::Ipv4Addr, ptr::addr_of_mut};

use embassy_net::{
    IpAddress,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_time::{Duration, Timer};
use esp_hal::spi::master::Address;
use esp_println::println;
use log::info;

const RX_BUFFER_SIZE: usize = 4096;
const TX_BUFFER_SIZE: usize = 512;
static mut RX_META_BUFFER: [PacketMetadata; 2] = [PacketMetadata::EMPTY; _];
static mut RX_BUFFER: [u8; RX_BUFFER_SIZE] = [0; RX_BUFFER_SIZE];
static mut TX_META_BUFFER: [PacketMetadata; 2] = [PacketMetadata::EMPTY; _];
static mut TX_BUFFER: [u8; TX_BUFFER_SIZE] = [0; TX_BUFFER_SIZE];

#[embassy_executor::task]
pub async fn start_discovery_server(stack: embassy_net::Stack<'static>) {
    let mut socket = UdpSocket::new(
        stack,
        unsafe { &mut *addr_of_mut!(RX_META_BUFFER) },
        unsafe { &mut *addr_of_mut!(RX_BUFFER) },
        unsafe { &mut *addr_of_mut!(TX_META_BUFFER) },
        unsafe { &mut *addr_of_mut!(TX_BUFFER) },
    );

    let endpoint = (IpAddress::Ipv4(Ipv4Addr::new(224, 0, 2, 60)), 4445);
    assert!(endpoint.0.is_multicast());

    stack
        .join_multicast_group(endpoint.0)
        .expect("failed to join minecraft multicast");

    socket
        .bind((stack.config_v4().unwrap().address.address(), 4445))
        .expect("failed to bind to minecraft multicast");

    let message = "[MOTD]block-chain - a block game on your keychain![/MOTD][AD]25565[/AD]".as_bytes();

    socket.wait_send_ready().await;

    loop {
        let _ = socket.send_to(message, endpoint).await;
        Timer::after(Duration::from_millis(5_000)).await;
    }
}
