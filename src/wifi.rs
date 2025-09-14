use embassy_net::Runner;
use embassy_time::{Duration, Timer};
use esp_wifi::wifi::{ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiState};
use log::{info, warn};

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

#[embassy_executor::task]
pub async fn maintain_wifi_connection(mut controller: WifiController<'static>) {
    info!("device capabilities: {:?}", controller.capabilities());

    loop {
        match esp_wifi::wifi::wifi_state() {
            WifiState::StaConnected => {
                // wait for disconnect
                controller.wait_for_event(WifiEvent::StaDisconnected).await;
                Timer::after(Duration::from_millis(5000)).await
            }
            _ => {}
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::Client(ClientConfiguration {
                ssid: SSID.into(),
                password: PASSWORD.into(),
                ..Default::default()
            });
            controller
                .set_configuration(&client_config)
                .expect("failed to set wifi config");
            controller
                .start_async()
                .await
                .expect("failed to start wifi connect");
            info!("started wifi connect");
        }

        match controller.connect_async().await {
            Ok(_) => info!("wifi connected"),
            Err(e) => {
                warn!("failed to connect to wifi: {e:?}");
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}