use core::ptr::addr_of_mut;

use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_bootloader_esp_idf::partitions::{
    DataPartitionSubType, FlashRegion, PartitionEntry, PartitionTable, PartitionType,
};
use esp_storage::FlashStorage;
use log::{info, warn};
use static_cell::StaticCell;

use crate::world::block::{BlockUpdate, BlockUpdatePointer, PackedChunkPosition};
use embedded_storage::nor_flash::NorFlashError;

pub mod block;

static FLASH_STORAGE: StaticCell<FlashStorage> = StaticCell::new();
static WORLD_PARTITION_ENTRY: StaticCell<PartitionEntry<'static>> = StaticCell::new();
static mut FLASH_BUFFER: [u8; esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN] = [0; _];

const READ_ALIGNMENT: usize = 4;
const CHUNKED_READ_ALIGNMENT: usize = READ_ALIGNMENT * 32;
const BLOCK_UPDATE_SIZE: u32 = core::mem::size_of::<BlockUpdate>() as u32;

type WorldPositionType = u8; // 256 * 16 blocks

/**
 * On the flash, we have the first X bits that determine whether or not that position is filled
 * Then, a bunch of end-to-end blockupdates that report being filled to the 'fill map'
 *
 * Poorly explained but I'm a little tired right now
 */

pub struct World {
    flash: FlashRegion<'static, FlashStorage>,
    max_update_count: u32,
    reserved_data_offset_bytes: u32,
    reserved_fill_offset_bytes: u32,
    data_offset_bytes: u32,
}

impl World {
    pub fn new() -> Self {
        let storage = FLASH_STORAGE.init(FlashStorage::new());
        let pt: PartitionTable<'static> =
            esp_bootloader_esp_idf::partitions::read_partition_table(storage, unsafe {
                &mut *addr_of_mut!(FLASH_BUFFER)
            })
            .expect("failed to fetch partition table");

        let pt_entry = WORLD_PARTITION_ENTRY.init(
            pt.find_partition(PartitionType::Data(DataPartitionSubType::Undefined))
                .expect("failed to search pt")
                .expect("failed to find world data"),
        );
        let mut world: FlashRegion<'static, FlashStorage> = pt_entry.as_embedded_storage(storage);

        let partition_size = pt_entry.len();

        let block_updates = (partition_size * 8).div_floor(8 * BLOCK_UPDATE_SIZE + 1);
        let fill_marker_length = block_updates.div_floor(8); // Floor it, drop the leftover space (even if we can fit stuff, I can't think of a way to calculate this reliably)
        let block_updates = fill_marker_length * 8; // Round block updates to the nearest byte for the fill marker, makes things simpler

        let total = block_updates * BLOCK_UPDATE_SIZE + fill_marker_length;

        if total > partition_size {
            panic!("something is wrong in the world size caculations");
        }

        if option_env!("RESET_WORLD").is_some() {
            info!("clearing world...");
            let zero = [0; READ_ALIGNMENT];
            for offset in (0..partition_size).step_by(READ_ALIGNMENT) {
                world
                    .write(
                        offset,
                        &zero[0..READ_ALIGNMENT
                            .min((partition_size - (offset + READ_ALIGNMENT as u32)) as usize)],
                    )
                    .expect("failed to write to flash");
            }
            info!("cleared world");
        }

        let mut reserved: u32 = WorldPositionType::MAX.into();
        reserved += 1;
        reserved *= reserved;
        let reserve_aligned_bytes = reserved.div_ceil(8);

        if reserved >= block_updates {
            panic!("requires more reserved block updates than available");
        }

        let data_offset = fill_marker_length + (reserved * BLOCK_UPDATE_SIZE);
        info!(
            "total world size: {total} of {partition_size} [{fill_marker_length}b fill map then {}b data, {block_updates} updates of {}b]. losing {} bytes. unreserved data starts at {data_offset}",
            block_updates * BLOCK_UPDATE_SIZE,
            BLOCK_UPDATE_SIZE,
            partition_size - total
        );

        Self {
            flash: world,
            max_update_count: block_updates,
            reserved_data_offset_bytes: fill_marker_length, // End of the fill markers
            reserved_fill_offset_bytes: reserve_aligned_bytes,
            data_offset_bytes: data_offset,
        }
    }

    pub fn find_free_space(&mut self) -> BlockUpdatePointer {
        let mut bytes = [0u8; CHUNKED_READ_ALIGNMENT];

        // From the start of the reserved fill markers to the end of the fill markers (start of data)
        for offset in (self.reserved_fill_offset_bytes..self.reserved_data_offset_bytes)
            .step_by(CHUNKED_READ_ALIGNMENT)
        {
            self.flash
                .read(offset as u32, &mut bytes)
                .expect("failed to read");

            let to_use =
                CHUNKED_READ_ALIGNMENT.min((self.reserved_data_offset_bytes - offset) as usize);

            // If one of them has 0s
            for byte_i in 0..to_use {
                if bytes[byte_i] != u8::MAX {
                    for v in 0..8 {
                        if bytes[byte_i] & (0b1 << v) == 0 {
                            return BlockUpdatePointer::from_u32(offset * 8 + byte_i as u32);
                        }
                    }
                }
            }
        }

        panic!("no space left in the world");
    }

    fn mark_space_filled(&mut self, pointer: u32) {
        let pointer = pointer - self.reserved_data_offset_bytes;
        let byte_offset = pointer.div_floor(8);
        let bit_offset = pointer - byte_offset * 8;

        let mut buf = [0u8; READ_ALIGNMENT];
        self.flash
            .read(byte_offset, &mut buf)
            .expect("failed to read from flash");
        buf[0] |= 0b1 << bit_offset;
        self.flash
            .write(byte_offset, &buf)
            .expect("failed to write");
    }

    pub fn set_chunk_start(x: WorldPositionType, z: WorldPositionType, data: BlockUpdate) {}

    pub fn read_block_update(pointer: BlockUpdatePointer) {}

    pub fn write_block_update(&mut self, pointer: BlockUpdatePointer, value: BlockUpdate) {
        let memory_offset = pointer.to_u32() * BLOCK_UPDATE_SIZE;
        let raw_bytes: [u8; BLOCK_UPDATE_SIZE as usize] = unsafe { core::mem::transmute(value) };
        self.flash
            .write(memory_offset, &raw_bytes)
            .expect("failed to write data to flash");
        self.mark_space_filled(memory_offset);
    }
}
