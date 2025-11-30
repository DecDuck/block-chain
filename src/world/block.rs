use core::u16;

use log::info;

use crate::world::WorldPositionType;

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
    NONE = 0,
    AIR = 1,
    STONE = 2,
    DIRT = 3,

}

#[repr(C, packed(1))]
#[derive(Clone, Copy)]
pub struct BlockUpdatePointer([u8; 3]);

impl BlockUpdatePointer {
    pub fn from_u32(value: u32) -> Self {
        let [n1, n2, n3, _] = value.to_le_bytes();
        Self([n1, n2, n3])
    }

    pub fn to_u32(self) -> u32 {
        let value = u32::from_le_bytes([self.0[0], self.0[1], self.0[2], 0]);
        value
    }
}

#[repr(C, packed(1))]
pub struct BlockUpdate {
    pub pos: PackedChunkPosition,
    pub block: BlockType,
    pub next: BlockUpdatePointer,
    pub chunk_x: WorldPositionType,
    pub chunk_z: WorldPositionType,
}