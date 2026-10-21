//! Wi-Fi and MQTT on their own thread (pinned to core 0, away from the render loop), delivering messages
//! to the render loop through a shared [`Mailbox`].
//!
//! Nothing here can stop the eyes or give up for good: without credentials the network never starts; a
//! failed join is retried with backoff; esp-mqtt reconnects by itself; and if the driver or client cannot
//! even be created, the whole network stack is torn down and rebuilt after a backoff.

use std::convert::Infallible;
use std::ffi::CStr;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_idf_svc::mqtt::client::{
    Details, EspMqttClient, EventPayload, LwtConfiguration, MqttClientConfiguration, QoS,
};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{
    AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi, PmfConfiguration,
};
use eyes_core::topics;
use eyes_hal::{Delivery, Inbox, Mailbox, Message};
use log::{error, info, warn};

use crate::provision::{MqttSettings, Provisioning, WifiCredentials};

/// Ten seconds of readings at the sensor's 10 Hz; more than the render loop ever falls behind.
const DISTANCE_QUEUE: usize = 16;
/// Comfortably above any valid config object; larger payloads arrive in chunks and are ignored.
const MQTT_BUFFER_BYTES: usize = 1024;
const CLIENT_ID: &str = "halloween-eyes";
const LINK_CHECK_PERIOD: Duration = Duration::from_secs(2);
const RETRY_BACKOFF: (Duration, Duration) = (Duration::from_secs(2), Duration::from_secs(60));
/// flight-rs brought Wi-Fi up on a 16 KiB main stack; 12 KiB without TLS, and the high-water mark is
/// logged so a bench run shows the real margin.
const NET_STACK_BYTES: usize = 12 * 1024;
const NET_THREAD_NAME: &CStr = c"net";
const STACK_REPORT_PERIOD: Duration = Duration::from_secs(300);

type SharedMailbox = Arc<Mutex<Mailbox>>;

pub struct SharedInbox(SharedMailbox);

impl Inbox for SharedInbox {
    fn poll(&mut self) -> Option<Message> {
        lock(&self.0).poll()
    }
}

/// A panic elsewhere while holding the lock must not take the eyes down with it; the mailbox is always
/// left consistent, so carry on with its contents.
fn lock(mailbox: &SharedMailbox) -> MutexGuard<'_, Mailbox> {
    mailbox
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Start the network thread and return the inbox it feeds. Never fails: on any problem the inbox simply
/// stays empty and the eyes idle.
pub fn start(
    modem: Modem<'static>,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    provisioning: &Provisioning,
) -> SharedInbox {
    let mailbox: SharedMailbox = Arc::new(Mutex::new(Mailbox::new(DISTANCE_QUEUE)));
    let Some(wifi) = provisioning.wifi else {
        warn!("no Wi-Fi credentials provisioned; running offline");
        return SharedInbox(mailbox);
    };
    let mqtt = provisioning.mqtt;
    let thread_mailbox = mailbox.clone();
    let pinned = ThreadSpawnConfiguration {
        name: Some(NET_THREAD_NAME),
        stack_size: NET_STACK_BYTES,
        pin_to_core: Some(Core::Core0),
        ..Default::default()
    };
    if let Err(reason) = pinned.set() {
        warn!("could not pin the network thread to core 0: {reason}");
    }
    let spawned = thread::Builder::new()
        .stack_size(NET_STACK_BYTES)
        .spawn(move || supervise(modem, sysloop, nvs, wifi, mqtt, thread_mailbox));
    if let Err(reason) = ThreadSpawnConfiguration::default().set() {
        warn!("could not restore the default thread configuration: {reason}");
    }
    if let Err(reason) = spawned {
        error!("could not start the network thread: {reason}; running offline");
    }
    SharedInbox(mailbox)
}

/// Runs the network forever, rebuilding it from scratch whenever it fails.
fn supervise(
    mut modem: Modem<'static>,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    wifi: WifiCredentials,
    mqtt: Option<MqttSettings>,
    mailbox: SharedMailbox,
) {
    let mut backoff = RETRY_BACKOFF.0;
    loop {
        // SAFETY: each attempt's Wi-Fi driver is dropped when `run` returns, before the next reborrow,
        // so the modem is never driven twice at once.
        let modem = unsafe { modem.reborrow() };
        let Err(reason) = run(
            modem,
            sysloop.clone(),
            nvs.clone(),
            &wifi,
            mqtt.as_ref(),
            &mailbox,
        );
        error!(
            "network failed ({reason:?}); rebuilding it in {}s",
            backoff.as_secs()
        );
        thread::sleep(backoff);
        backoff = (backoff * 2).min(RETRY_BACKOFF.1);
    }
}

fn run(
    modem: Modem<'_>,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    credentials: &WifiCredentials,
    mqtt: Option<&MqttSettings>,
    mailbox: &SharedMailbox,
) -> anyhow::Result<Infallible> {
    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs))?, sysloop)?;
    wifi.set_configuration(&Configuration::Client(station_config(credentials)?))?;
    wifi.start()?;

    // Kept alive here so `recv_timeout` below blocks even when MQTT is not configured.
    let (connected_tx, connected_rx) = mpsc::channel();
    let mut client = match mqtt {
        Some(settings) => Some(connect_mqtt(
            settings,
            mailbox.clone(),
            connected_tx.clone(),
        )?),
        None => {
            warn!("no MQTT broker provisioned; Wi-Fi only");
            None
        }
    };

    let mut backoff = RETRY_BACKOFF.0;
    let mut last_stack_report: Option<Instant> = None;
    loop {
        if last_stack_report.is_none_or(|at| at.elapsed() >= STACK_REPORT_PERIOD) {
            last_stack_report = Some(Instant::now());
            log_stack_high_water();
        }
        if !wifi.is_connected().unwrap_or(false) {
            match wifi.connect().and_then(|()| wifi.wait_netif_up()) {
                Ok(()) => {
                    info!("joined Wi-Fi '{}'", credentials.ssid);
                    backoff = RETRY_BACKOFF.0;
                }
                Err(reason) => {
                    warn!(
                        "Wi-Fi join failed ({reason}); retrying in {}s",
                        backoff.as_secs()
                    );
                    thread::sleep(backoff);
                    backoff = (backoff * 2).min(RETRY_BACKOFF.1);
                    continue;
                }
            }
        }
        if connected_rx.recv_timeout(LINK_CHECK_PERIOD).is_ok() {
            if let Some(client) = client.as_mut() {
                announce(client);
            }
        }
    }
}

fn log_stack_high_water() {
    // SAFETY: a null handle means the calling task.
    // ESP-IDF's FreeRTOS counts stack in bytes.
    let unused = unsafe { esp_idf_svc::sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()) };
    info!("net thread stack: {unused} B never used of {NET_STACK_BYTES} B");
}

fn station_config(credentials: &WifiCredentials) -> anyhow::Result<ClientConfiguration> {
    // build.rs rejects over-long values, so these only fail if the two ever disagree.
    let ssid = credentials
        .ssid
        .try_into()
        .map_err(|_| anyhow::anyhow!("Wi-Fi SSID is longer than 32 bytes"))?;
    let password = credentials
        .password
        .try_into()
        .map_err(|_| anyhow::anyhow!("Wi-Fi password is longer than 64 bytes"))?;
    Ok(ClientConfiguration {
        ssid,
        password,
        auth_method: if credentials.password.is_empty() {
            AuthMethod::None
        } else {
            AuthMethod::WPA2Personal
        },
        // Routers in WPA2/WPA3 transition mode reject stations that are not PMF-capable (found the hard way
        // in flight-rs).
        pmf_cfg: PmfConfiguration::Capable { required: false },
        ..Default::default()
    })
}

/// The client reconnects on its own; every (re)connect is signalled on `connected` so the network thread
/// can resubscribe (clean sessions forget subscriptions) and republish availability.
fn connect_mqtt(
    settings: &MqttSettings,
    mailbox: SharedMailbox,
    connected: Sender<()>,
) -> anyhow::Result<EspMqttClient<'static>> {
    let config = MqttClientConfiguration {
        client_id: Some(CLIENT_ID),
        username: settings.username,
        password: settings.password,
        keep_alive_interval: Some(Duration::from_secs(15)),
        reconnect_timeout: Some(Duration::from_secs(5)),
        buffer_size: MQTT_BUFFER_BYTES,
        lwt: Some(LwtConfiguration {
            topic: topics::STATUS,
            payload: topics::OFFLINE,
            qos: QoS::AtLeastOnce,
            retain: true,
        }),
        ..Default::default()
    };
    let client =
        EspMqttClient::new_cb(settings.url, &config, move |event| match event.payload() {
            EventPayload::Connected(_) => {
                info!("MQTT connected");
                let _ = connected.send(());
            }
            EventPayload::Received {
                topic: Some(topic),
                data,
                details: Details::Complete,
                ..
            } => match lock(&mailbox).deliver(topic, data) {
                Delivery::DroppedOldest => warn!("render loop behind; dropped the oldest distance"),
                Delivery::Queued | Delivery::ReplacedConfig | Delivery::Ignored => {}
            },
            EventPayload::Received { .. } => {
                warn!("ignoring an MQTT message larger than {MQTT_BUFFER_BYTES} B")
            }
            EventPayload::Disconnected => warn!("MQTT disconnected; esp-mqtt will reconnect"),
            EventPayload::Error(reason) => warn!("MQTT error: {reason:?}"),
            _ => {}
        })?;
    Ok(client)
}

fn announce(client: &mut EspMqttClient<'static>) {
    let results = [
        client.subscribe(topics::DISTANCE, QoS::AtMostOnce),
        client.subscribe(topics::CONFIG, QoS::AtLeastOnce),
        client.publish(topics::STATUS, QoS::AtLeastOnce, true, topics::ONLINE),
    ];
    for result in results {
        if let Err(reason) = result {
            warn!("MQTT subscribe/announce failed: {reason}");
        }
    }
}
