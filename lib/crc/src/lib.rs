//! Cyclic Redundancy Check implementation for PNG

#![cfg_attr(not(test), no_std)]

#[path = "_crc_table.rs"]
mod crc_table;
use crc_table::CRC_TABLE;

pub struct CRC {
    crc: u32,
}

impl CRC {
    pub fn new() -> Self {
        Self { crc: u32::MAX }
    }

    pub fn update(&mut self, buf: &[u8]) {
        self.crc = update_crc(self.crc, buf);
    }

    pub fn finalize(self) -> u32 {
        self.crc ^ u32::MAX
    }
}

#[inline(always)]
fn update_crc(crc: u32, buf: &[u8]) -> u32 {
    let mut c = crc;
    for &byte in buf {
        c = CRC_TABLE[(c as u8 ^ byte) as usize] ^ (c >> 8);
    }
    return c;
}
