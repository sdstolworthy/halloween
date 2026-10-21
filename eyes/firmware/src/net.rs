//! Wi-Fi and MQTT on their own thread, delivering messages to the render loop through a bounded channel.
//!
//! Nothing here can stop the eyes: without credentials the network never starts, a failed join is retried
//! with backoff, esp-mqtt reconnects by itself, and messages that arrive faster than the render loop
//! drains them are dropped (the distance is re-sent ten times a second anyway).

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError};
use std::thread;
use std::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::mqtt::client::{
    Details, EspMqttClient, EventPayload, LwtConfiguration, MqttClientConfiguration, QoS,
};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{
    AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi, PmfConfiguration,
};
use eyes_core::topics;
use eyes_hal::{Inbox, Message};
use log::{error, info, warn};

use crate::provision::{MqttSettings, Provisioning, WifiCredentials};

const INBOX_CAPACITY: usize = 32;
/// Comfortably above any valid config object; larger payloads arrive in chunks and are ignored.
const MQTT_BUFFER_BYTES: usize = 1024;
const CLIENT_ID: &str = "halloween-eyes";
const LINK_CHECK_PERIOD: Duration = Duration::from_secs(2);
const RETRY_BACKOFF: (Duration, Duration) = (Duration::from_secs(2), Duration::from_secs(60));
const NET_STACK_BYTES: usize = 8 * 1024;

pub struct ChannelInbox(Receiver<Message>);

impl Inbox for ChannelInbox {
    fn poll(&mut self) -> Option<Message> {
        self.0.try_recv().ok()
    }
}

/// Start the network thread and return the inbox it feeds. Never fails: on any problem the inbox simply
/// stays empty and the eyes idle.
pub fn start(
    modem: Modem<'static>,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    provisioning: &Provisioning,
) -> ChannelInbox {
    let (inbox_tx, inbox_rx) = mpsc::sync_channel(INBOX_CAPACITY);
    let Some(wifi) = provisioning.wifi else {
        warn!("no Wi-Fi credentials provisioned; running offline");
        return ChannelInbox(inbox_rx);
    };
    let mqtt = provisioning.mqtt;
    let spawned = thread::Builder::new()
        .name("net".into())
        .stack_size(NET_STACK_BYTES)
        .spawn(move || {
            if let Err(reason) = run(modem, sysloop, nvs, &wifi, mqtt.as_ref(), inbox_tx) {
                error!("network stopped: {reason:?}; the eyes keep running offline");
            }
        });
    if let Err(reason) = spawned {
        error!("could not start the network thread: {reason}");
    }
    ChannelInbox(inbox_rx)
}

fn run(
    modem: Modem<'static>,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    credentials: &WifiCredentials,
    mqtt: Option<&MqttSettings>,
    inbox: SyncSender<Message>,
) -> anyhow::Result<()> {
    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs))?, sysloop)?;
    wifi.set_configuration(&Configuration::Client(station_config(credentials)))?;
    wifi.start()?;

    // Kept alive here so `recv_timeout` below blocks even when MQTT is not configured.
    let (connected_tx, connected_rx) = mpsc::channel();
    let mut client = match mqtt {
        Some(settings) => Some(connect_mqtt(settings, inbox, connected_tx.clone())?),
        None => {
            warn!("no MQTT broker provisioned; Wi-Fi only");
            None
        }
    };

    let mut backoff = RETRY_BACKOFF.0;
    loop {
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
        match connected_rx.recv_timeout(LINK_CHECK_PERIOD) {
            Ok(()) => {
                if let Some(client) = client.as_mut() {
                    announce(client);
                }
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
        }
    }
}

fn station_config(credentials: &WifiCredentials) -> ClientConfiguration {
    ClientConfiguration {
        ssid: credentials.ssid.try_into().unwrap_or_default(),
        password: credentials.password.try_into().unwrap_or_default(),
        auth_method: if credentials.password.is_empty() {
            AuthMethod::None
        } else {
            AuthMethod::WPA2Personal
        },
        // Routers in WPA2/WPA3 transition mode reject stations that are not PMF-capable (found the hard way
        // in flight-rs).
        pmf_cfg: PmfConfiguration::Capable { required: false },
        ..Default::default()
    }
}

/// The client reconnects on its own; every (re)connect is signalled on `connected` so the network thread
/// can resubscribe (clean sessions forget subscriptions) and republish availability.
fn connect_mqtt(
    settings: &MqttSettings,
    inbox: SyncSender<Message>,
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
            } => {
                let message = Message {
                    topic: topic.to_owned(),
                    payload: data.to_vec(),
                };
                if let Err(TrySendError::Full(_)) = inbox.try_send(message) {
                    warn!("inbox full; dropped a message on {topic}");
                }
            }
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
