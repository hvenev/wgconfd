// Copyright 2019 Hristo Venev
//
// See COPYING.

#[inline]
pub fn i64_to_be(v: i64) -> [u8; 8] {
    u64_to_be(v as u64)
}

pub fn i64_from_be(v: [u8; 8]) -> i64 {
    u64_from_be(v) as i64
}

pub fn u64_to_be(v: u64) -> [u8; 8] {
    [
        (v >> 56) as u8,
        (v >> 48) as u8,
        (v >> 40) as u8,
        (v >> 32) as u8,
        (v >> 24) as u8,
        (v >> 16) as u8,
        (v >> 8) as u8,
        v as u8,
    ]
}

pub fn u64_from_be(v: [u8; 8]) -> u64 {
    (u64::from(v[0]) << 56)
        | (u64::from(v[1]) << 48)
        | (u64::from(v[2]) << 40)
        | (u64::from(v[3]) << 32)
        | (u64::from(v[4]) << 24)
        | (u64::from(v[5]) << 16)
        | (u64::from(v[6]) << 8)
        | u64::from(v[7])
}

pub fn u32_to_be(v: u32) -> [u8; 4] {
    [(v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8]
}

pub fn u32_from_be(v: [u8; 4]) -> u32 {
    (u32::from(v[0]) << 24) | (u32::from(v[1]) << 16) | (u32::from(v[2]) << 8) | u32::from(v[3])
}

pub fn u16_to_be(v: u16) -> [u8; 2] {
    [(v >> 8) as u8, v as u8]
}

pub fn u16_from_be(v: [u8; 2]) -> u16 {
    (u16::from(v[0]) << 8) | u16::from(v[1])
}
