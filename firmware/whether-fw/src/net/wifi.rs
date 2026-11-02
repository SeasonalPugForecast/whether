//! The Wi-Fi station: joins the configured network, reconnects with backoff, and
//! runs the embassy-net stack (DHCP and DNS).

use core::cell::Cell;
use core::sync::atomic::{AtomicBool, Ordering};

use critical_section::Mutex;
use fw_sleep::WifiHint;

use embassy_executor::{SpawnError, Spawner};
use embassy_net::{Runner, Stack, StackResources};
use embassy_time::Timer;
use esp_hal::peripherals::WIFI;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ControllerConfig, Interface, WifiController, WifiError,
};
use log::{info, warn};
use static_cell::StaticCell;

/// Whether the station is associated with the access point.
static LINK_UP: AtomicBool = AtomicBool::new(false);

/// The access point last joined, for the next wake to skip the scan.
static LAST_AP: Mutex<Cell<Option<WifiHint>>> = Mutex::new(Cell::new(None));

/// Sockets: DHCP, DNS and one TCP connection at a time.
const SOCKETS: usize = 3;

/// The longest wait between connection attempts.
const MAX_BACKOFF_S: u64 = 300;

/// The access point joined last, if any.
pub fn last_ap() -> Option<WifiHint> {
    critical_section::with(|cs| LAST_AP.borrow(cs).get())
}

/// Whether the station is associated (an IP address may still be pending).
pub fn link_up() -> bool {
    LINK_UP.load(Ordering::Relaxed)
}

/// Why the network could not be started.
#[derive(Debug)]
pub enum StartError {
    /// The network name or password is too long for the driver.
    Credentials,
    /// The Wi-Fi driver refused.
    Wifi(WifiError),
    /// A task could not be spawned.
    Spawn(SpawnError),
}

impl From<WifiError> for StartError {
    fn from(e: WifiError) -> Self {
        Self::Wifi(e)
    }
}

impl From<SpawnError> for StartError {
    fn from(e: SpawnError) -> Self {
        Self::Spawn(e)
    }
}

/// The station configuration: WPA2 (which also joins WPA2/WPA3 networks), or open
/// when the password is empty; with a hint, it joins that access point on that
/// channel without scanning.
fn station(ssid: &str, password: &str, hint: Option<WifiHint>) -> Result<Config, StartError> {
    let ssid = ssid.try_into().map_err(|_| StartError::Credentials)?;
    let authentication = if password.is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        AuthenticationMethodConfig::Wpa2Personal(
            password.try_into().map_err(|_| StartError::Credentials)?,
        )
    };
    let mut config = StationConfig::default()
        .with_ssid(ssid)
        .with_authentication(authentication);
    if let Some(hint) = hint {
        config = config.with_channel(hint.channel).with_bssid(hint.bssid);
    }
    Ok(Config::Station(config))
}

/// Starts Wi-Fi and the network stack, and spawns their tasks. `seed` randomizes
/// the stack (ports, DHCP transaction ids). With a `hint` (from the previous
/// wake) it first tries that access point directly, then scans as usual.
pub fn start(
    spawner: Spawner,
    wifi: WIFI<'static>,
    ssid: &str,
    password: &str,
    seed: u64,
    hint: Option<WifiHint>,
) -> Result<Stack<'static>, StartError> {
    static RESOURCES: StaticCell<StackResources<SOCKETS>> = StaticCell::new();
    let scan = station(ssid, password, None)?;
    let (config, fallback) = match hint {
        Some(hint) => (station(ssid, password, Some(hint))?, Some(scan)),
        None => (scan, None),
    };
    // The driver's default country is CN (channels 1–13); the device is in the US.
    // Each received frame takes a dynamic buffer (about 1.6 KB) from the heap until
    // the stack reads it; 16 instead of 32 bounds that, and TCP keeps at most 4 KB
    // in flight anyway.
    let controller_config = ControllerConfig::default()
        .with_country_info(*b"US")
        .with_dynamic_rx_buf_num(16);
    let mut controller = WifiController::new(wifi, controller_config)?;
    controller.set_config(&config)?;
    let (stack, runner) = embassy_net::new(
        Interface::station(),
        embassy_net::Config::dhcpv4(embassy_net::DhcpConfig::default()),
        RESOURCES.init(StackResources::new()),
        seed,
    );
    spawner.spawn(net_task(runner)?);
    spawner.spawn(connection(controller, fallback)?);
    Ok(stack)
}

/// Keeps the station connected: on failure or loss, retries after a delay that
/// doubles up to [`MAX_BACKOFF_S`]. If a remembered access point fails, it
/// switches to `fallback` (a normal scan) at once.
#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>, mut fallback: Option<Config>) {
    let mut backoff_s = 2;
    loop {
        LINK_UP.store(false, Ordering::Relaxed);
        match controller.connect_async().await {
            Ok(connected) => {
                info!(
                    "Wi-Fi associated (channel {}, {:?})",
                    connected.channel, connected.authmode
                );
                let hint = WifiHint {
                    channel: connected.channel,
                    bssid: connected.bssid,
                };
                critical_section::with(|cs| LAST_AP.borrow(cs).set(Some(hint)));
                LINK_UP.store(true, Ordering::Relaxed);
                backoff_s = 2;
                match controller.wait_for_disconnect_async().await {
                    Ok(lost) => warn!("Wi-Fi lost: {:?}", lost.reason),
                    Err(e) => warn!("Wi-Fi state unknown: {e:?}"),
                }
            }
            Err(e) => {
                if let Some(scan) = fallback.take() {
                    warn!("the remembered access point did not answer ({e:?}); scanning");
                    if let Err(e) = controller.set_config(&scan) {
                        warn!("Wi-Fi configuration refused: {e:?}");
                    }
                    continue;
                }
                warn!("Wi-Fi connection failed: {e:?}; retrying in {backoff_s} s");
                Timer::after_secs(backoff_s).await;
                backoff_s = (backoff_s * 2).min(MAX_BACKOFF_S);
            }
        }
    }
}

/// Runs the network stack.
#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) -> ! {
    runner.run().await
}
