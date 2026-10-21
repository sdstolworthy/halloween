//! In-memory implementations of the HAL traits for host tests. Each is a cheap handle onto shared state,
//! so a test can keep a clone to drive or inspect it after moving the original into an [`crate::App`].

use alloc::collections::VecDeque;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use core::convert::Infallible;

use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use eyes_core::layout::{PANEL_HEIGHT, PANEL_WIDTH};

use crate::{Clock, Inbox, Matrix, Message};

#[derive(Clone, Default)]
pub struct FakeClock(Rc<Cell<u64>>);

impl FakeClock {
    pub fn set(&self, now_ms: u64) {
        self.0.set(now_ms);
    }
}

impl Clock for FakeClock {
    fn uptime_ms(&self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Default)]
pub struct QueueInbox(Rc<RefCell<VecDeque<Message>>>);

impl QueueInbox {
    pub fn push(&self, topic: &str, payload: &[u8]) {
        self.0.borrow_mut().push_back(Message {
            topic: topic.to_string(),
            payload: payload.to_vec(),
        });
    }

    pub fn pending(&self) -> usize {
        self.0.borrow().len()
    }
}

impl Inbox for QueueInbox {
    fn poll(&mut self) -> Option<Message> {
        self.0.borrow_mut().pop_front()
    }
}

struct MatrixState {
    back: Vec<Rgb888>,
    presented: Vec<Rgb888>,
    flushes: usize,
    brightness_writes: Vec<u8>,
}

/// A 128x64 [`Matrix`] with a back buffer that `flush` copies to the presented frame.
#[derive(Clone)]
pub struct MockMatrix(Rc<RefCell<MatrixState>>);

impl MockMatrix {
    pub fn new() -> Self {
        let pixels = (PANEL_WIDTH * PANEL_HEIGHT) as usize;
        Self(Rc::new(RefCell::new(MatrixState {
            back: vec![Rgb888::BLACK; pixels],
            presented: vec![Rgb888::BLACK; pixels],
            flushes: 0,
            brightness_writes: Vec::new(),
        })))
    }

    pub fn flushes(&self) -> usize {
        self.0.borrow().flushes
    }

    pub fn lit_presented(&self) -> usize {
        self.0
            .borrow()
            .presented
            .iter()
            .filter(|&&pixel| pixel != Rgb888::BLACK)
            .count()
    }

    /// Every brightness level written, in order.
    pub fn brightness_writes(&self) -> Vec<u8> {
        self.0.borrow().brightness_writes.clone()
    }
}

impl Default for MockMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl OriginDimensions for MockMatrix {
    fn size(&self) -> Size {
        Size::new(PANEL_WIDTH, PANEL_HEIGHT)
    }
}

impl DrawTarget for MockMatrix {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Infallible>
    where
        I: IntoIterator<Item = Pixel<Rgb888>>,
    {
        let mut state = self.0.borrow_mut();
        for Pixel(point, colour) in pixels {
            if (0..PANEL_WIDTH as i32).contains(&point.x)
                && (0..PANEL_HEIGHT as i32).contains(&point.y)
            {
                state.back[(point.y as u32 * PANEL_WIDTH + point.x as u32) as usize] = colour;
            }
        }
        Ok(())
    }
}

impl Matrix for MockMatrix {
    fn flush(&mut self) -> Result<(), Infallible> {
        let mut state = self.0.borrow_mut();
        let back = state.back.clone();
        state.presented = back;
        state.flushes += 1;
        Ok(())
    }

    fn set_brightness(&mut self, level: u8) {
        self.0.borrow_mut().brightness_writes.push(level);
    }
}
