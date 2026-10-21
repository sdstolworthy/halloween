//! Where MQTT messages wait between the network thread and the frame loop.
//!
//! The retained config and mode are state, not events: only the newest of each matters and neither may
//! be lost to a burst of distance readings, so each has its own latest-wins slot. Distance readings keep their order
//! (the debounce counts consecutive ones) in a bounded queue that drops the oldest when full, since a
//! fresh reading is worth more than a stale one.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use eyes_core::topics::{self, Topic};

use crate::{Inbox, Message};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Queued,
    /// The config or mode slot already held an unread message; it was replaced.
    ReplacedUnread,
    /// The distance queue was full; its oldest reading was dropped to make room.
    DroppedOldest,
    /// Not a topic the eyes handle.
    Ignored,
}

pub struct Mailbox {
    config: Option<Vec<u8>>,
    mode: Option<Vec<u8>>,
    distances: VecDeque<Vec<u8>>,
    capacity: usize,
}

impl Mailbox {
    pub fn new(distance_capacity: usize) -> Self {
        Self {
            config: None,
            mode: None,
            distances: VecDeque::with_capacity(distance_capacity),
            capacity: distance_capacity.max(1),
        }
    }

    pub fn deliver(&mut self, topic: &str, payload: &[u8]) -> Delivery {
        match topics::route(topic) {
            Some(Topic::Config) => replace(&mut self.config, payload),
            Some(Topic::Mode) => replace(&mut self.mode, payload),
            Some(Topic::Distance) => {
                let dropped = self.distances.len() >= self.capacity;
                if dropped {
                    self.distances.pop_front();
                }
                self.distances.push_back(payload.to_vec());
                if dropped {
                    Delivery::DroppedOldest
                } else {
                    Delivery::Queued
                }
            }
            None => Delivery::Ignored,
        }
    }
}

fn replace(slot: &mut Option<Vec<u8>>, payload: &[u8]) -> Delivery {
    match slot.replace(payload.to_vec()) {
        Some(_) => Delivery::ReplacedUnread,
        None => Delivery::Queued,
    }
}

impl Inbox for Mailbox {
    fn poll(&mut self) -> Option<Message> {
        let (topic, payload) = if let Some(payload) = self.config.take() {
            (topics::CONFIG, payload)
        } else if let Some(payload) = self.mode.take() {
            (topics::MODE, payload)
        } else {
            (topics::DISTANCE, self.distances.pop_front()?)
        };
        Some(Message {
            topic: topic.into(),
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;

    fn drain(mailbox: &mut Mailbox) -> Vec<(String, Vec<u8>)> {
        core::iter::from_fn(|| mailbox.poll())
            .map(|message| (message.topic, message.payload))
            .collect()
    }

    #[test]
    fn config_survives_a_full_distance_queue() {
        let mut mailbox = Mailbox::new(2);
        mailbox.deliver(topics::CONFIG, b"{}");
        for _ in 0..10 {
            mailbox.deliver(topics::DISTANCE, b"1.00");
        }
        assert_eq!(
            drain(&mut mailbox)[0],
            (topics::CONFIG.into(), b"{}".to_vec())
        );
    }

    #[test]
    fn only_the_newest_config_is_kept() {
        let mut mailbox = Mailbox::new(2);
        mailbox.deliver(topics::CONFIG, b"{\"brightness\": 10}");
        let delivery = mailbox.deliver(topics::CONFIG, b"{\"brightness\": 20}");
        assert_eq!(
            (delivery, drain(&mut mailbox)),
            (
                Delivery::ReplacedUnread,
                vec![(topics::CONFIG.into(), b"{\"brightness\": 20}".to_vec())]
            )
        );
    }

    #[test]
    fn a_full_distance_queue_drops_the_oldest_reading() {
        let mut mailbox = Mailbox::new(2);
        let deliveries: Vec<Delivery> = [b"1", b"2", b"3"]
            .iter()
            .map(|payload| mailbox.deliver(topics::DISTANCE, *payload))
            .collect();
        let payloads: Vec<Vec<u8>> = drain(&mut mailbox)
            .into_iter()
            .map(|(_, payload)| payload)
            .collect();
        assert_eq!(
            (deliveries, payloads),
            (
                vec![Delivery::Queued, Delivery::Queued, Delivery::DroppedOldest],
                vec![b"2".to_vec(), b"3".to_vec()]
            )
        );
    }

    #[test]
    fn mode_survives_a_full_distance_queue_and_only_the_newest_is_kept() {
        let mut mailbox = Mailbox::new(2);
        mailbox.deliver(topics::MODE, b"evil");
        mailbox.deliver(topics::MODE, b"normal");
        for _ in 0..10 {
            mailbox.deliver(topics::DISTANCE, b"1.00");
        }
        let modes: Vec<Vec<u8>> = drain(&mut mailbox)
            .into_iter()
            .filter(|(topic, _)| topic == topics::MODE)
            .map(|(_, payload)| payload)
            .collect();
        assert_eq!(modes, vec![b"normal".to_vec()]);
    }

    #[test]
    fn other_topics_are_ignored() {
        let mut mailbox = Mailbox::new(2);
        let delivery = mailbox.deliver(topics::STATUS, b"online");
        assert_eq!((delivery, mailbox.poll()), (Delivery::Ignored, None));
    }
}
