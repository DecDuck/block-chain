#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(type_alias_impl_trait)]
#![feature(generic_const_exprs)]

mod packet;
mod server;
mod wifi;
mod utils;
mod encryption;

#[macro_use]
extern crate build_const;
extern crate alloc;

use embassy_executor::Spawner;
use embassy_net::{StackResources, tcp::TcpSocket};
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock, gpio::{Level, Output, OutputConfig}, rng::Rng, rsa::Rsa, timer::{systimer::SystemTimer, timg::TimerGroup}
};
use esp_wifi::EspWifiController;
use log::{info, warn};

use crate::{
    encryption::ServerEncryption, server::start_tcp_server, wifi::{maintain_wifi_connection, net_task}
};

esp_bootloader_esp_idf::esp_app_desc!();

macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

#[embassy_executor::task]
async fn run(mut output: Output<'static>) {
    loop {
        Timer::after(Duration::from_millis(1_000)).await;
        output.toggle();
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // 72 kB
    esp_alloc::heap_allocator!(size: 72 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut rng = Rng::new(peripherals.RNG);
    let rsa = Rsa::new(peripherals.RSA).into_async();

    let encryption = mk_static!(ServerEncryption<'static>, ServerEncryption::new(rsa, rng));

    let esp_radio_ctrl = &*mk_static!(
        EspWifiController<'static>,
        esp_wifi::init(timg0.timer0, rng).expect("failed to init radio")
    );

    let (mut controller, interfaces) =
        esp_wifi::wifi::new(esp_radio_ctrl, peripherals.WIFI).expect("failed to start wifi");

    let wifi_interface = interfaces.sta;

    controller
        .set_power_saving(esp_wifi::config::PowerSaveMode::None)
        .expect("failed to set power saving");

    let systimer = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(systimer.alarm0);

    let config = embassy_net::Config::dhcpv4(Default::default());

    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let (stack, runner) = embassy_net::new(
        wifi_interface,
        config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    spawner
        .spawn(maintain_wifi_connection(controller))
        .expect("failed to spawn connection");
    spawner
        .spawn(net_task(runner))
        .expect("failed to start wifi runner");

    let led = Output::new(peripherals.GPIO8, Level::Low, OutputConfig::default());
    spawner.spawn(run(led)).ok();

    while !stack.is_link_up() {
        Timer::after(Duration::from_millis(500)).await
    }

    loop {
        if let Some(config) = stack.config_v4() {
            info!("Got IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    spawner
        .spawn(start_tcp_server(stack, encryption))
        .expect("failed to start tcp server");

    loop {
        Timer::after(Duration::from_millis(5_000)).await;
    }
}
