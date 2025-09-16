use core::u16;

use log::info;

pub struct PackedChunkPosition(u16);

impl PackedChunkPosition {
    pub fn x(&self) -> u8 {
        // Little Endian
        (self.0 & 0b0000000000001111) as u8
    }
    pub fn z(&self) -> u8 {
        ((self.0 & 0b0000000011110000) >> 4) as u8
    }
    pub fn y(&self) -> u8 {
        ((self.0 & 0b1111111100000000) >> 8) as u8
    }

    pub fn new(x: u8, y: u8, z: u8) -> Self {
        let mut v: u16 = 0;
        v |= x as u16;
        v |= (z << 4) as u16;
        let mut v_y: u16 = y.into();
        v_y <<= 8;
        v |= v_y;
        PackedChunkPosition(v)
    }
}

#[repr(u16)]
pub enum BlockType {
    AIR = 0,
}

#[repr(C, packed(1))]
pub struct BlockUpdate {
    pos: PackedChunkPosition,
    block: BlockType,
    run_length: u16,
    next: *mut BlockUpdate
}
