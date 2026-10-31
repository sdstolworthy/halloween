//! Bench portal: one embedded page on port 80 that simulates the porch sensor and the scare
//! automation. Its two POST endpoints deliver raw payloads into the same [`Mailbox`] the MQTT
//! client feeds, on the same topics, so a simulated walk-up exercises the device's real payload
//! parsing, distance queue, stale timeout and mode handling — not a parallel code path.
//!
//! The portal is unauthenticated by design: it only exists on the Halloween LAN and only injects
//! the two inputs the eyes already trust from MQTT. The ESP-IDF httpd runs its own tasks; the
//! handlers below only take the mailbox lock for a few bounded writes, which is the same
//! contention the MQTT callback already creates for the render loop.

use esp_idf_svc::http::server::{Configuration, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;
use eyes_core::topics;
use log::info;

use crate::net::{lock, SharedMailbox};

const PORTAL_HTML: &str = include_str!("portal.html");
/// Distance payloads are "d.dd" or "none"; mode payloads are "evil"/"normal". Anything longer is
/// not a payload the eyes accept, so it is cut off and rejected by the device's own parsers.
const MAX_BODY: usize = 16;
/// The default 4 KiB session stack overflows building responses; 6 KiB holds comfortably.
const HTTPD_STACK: usize = 6 * 1024;

pub fn start(mailbox: SharedMailbox) -> anyhow::Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&Configuration {
        stack_size: HTTPD_STACK,
        ..Default::default()
    })?;

    server.fn_handler::<anyhow::Error, _>("/", Method::Get, |request| {
        let mut response = request.into_response(
            200,
            Some("OK"),
            &[("Content-Type", "text/html; charset=utf-8")],
        )?;
        response.write_all(PORTAL_HTML.as_bytes())?;
        Ok(())
    })?;

    let distance_mailbox = mailbox.clone();
    server.fn_handler::<anyhow::Error, _>("/simulate/distance", Method::Post, move |request| {
        inject(&distance_mailbox, topics::DISTANCE, request)
    })?;

    server.fn_handler::<anyhow::Error, _>("/simulate/mode", Method::Post, move |request| {
        inject(&mailbox, topics::MODE, request)
    })?;

    info!(
        "bench portal on port 80 (simulates {} and {})",
        topics::DISTANCE,
        topics::MODE
    );
    Ok(server)
}

fn inject(
    mailbox: &SharedMailbox,
    topic: &str,
    mut request: esp_idf_svc::http::server::Request<
        &mut esp_idf_svc::http::server::EspHttpConnection<'_>,
    >,
) -> anyhow::Result<()> {
    let mut body = [0u8; MAX_BODY];
    let mut filled = 0;
    while filled < body.len() {
        let read = request.read(&mut body[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    let delivery = lock(mailbox).deliver(topic, &body[..filled]);
    let mut response = request.into_ok_response()?;
    response.write_all(format!("{delivery:?}\n").as_bytes())?;
    Ok(())
}
