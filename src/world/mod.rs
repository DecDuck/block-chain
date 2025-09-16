use core::ptr::addr_of_mut;

use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_bootloader_esp_idf::partitions::{
    DataPartitionSubType, FlashRegion, PartitionEntry, PartitionTable, PartitionType,
};
use esp_storage::FlashStorage;
use log::info;
use static_cell::StaticCell;

use crate::world::block::{BlockUpdate, PackedChunkPosition};

mod block;

static FLASH_STORAGE: StaticCell<FlashStorage> = StaticCell::new();
static WORLD_PARTITION_ENTRY: StaticCell<PartitionEntry<'static>> = StaticCell::new();
static mut FLASH_BUFFER: [u8; esp_bootloader_esp_idf::partitions::PARTITION_TABLE_MAX_LEN] = [0; _];

const READ_ALIGNMENT: usize = 64;

/**
 * On the flash, we have the first X bits that determine whether or not that position is filled
 * Then, a bunch of end-to-end blockupdates that report being filled to the 'fill map'
 *
 * Poorly explained but I'm a little tired right now
 */

pub struct World {
    flash: FlashRegion<'static, FlashStorage>,
    max_update_count: u32,
    data_offset: u32,
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
        let update_size = core::mem::size_of::<BlockUpdate>() as u32;

        let block_updates = (partition_size * 8) / (8 * update_size + 1);
        let fill_marker_length = block_updates.div_floor(8);
        let block_updates = fill_marker_length * 8; // Round block updates to the nearest byte for the fill marker, makes things simpler

        let total = block_updates * update_size + fill_marker_length;

        if total > partition_size {
            panic!("something is wrong in the world size caculations");
        }

        info!(
            "total world size: {} of {} [{} fill map then {} data, {} updates]. losing {} bytes",
            total,
            partition_size,
            fill_marker_length,
            block_updates * update_size,
            block_updates,
            partition_size - total
        );

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
        }

        Self {
            flash: world,
            max_update_count: block_updates,
            data_offset: fill_marker_length,
        }
    }

    pub fn calculate_remaining_updates(&mut self) -> (u32, u32) {
        let mut remaining = 0;
        let mut total = 0;

        let mut bytes = [0u8; READ_ALIGNMENT];

        for offset in (0..self.data_offset).step_by(READ_ALIGNMENT) {
            self.flash
                .read(offset as u32, &mut bytes)
                .expect("failed to read");

            let to_use = READ_ALIGNMENT.min((self.data_offset - offset) as usize);

            // If one of them has 0s
            for byte_i in 0..to_use {
                if bytes[byte_i] != u8::MAX {
                    for v in 0..8 {
                        if bytes[byte_i] & (0b1 << v) == 0 {
                            remaining += 1;
                        }
                        total += 1;
                    }
                }
            }
        }

        info!("{}", self.max_update_count);

        (remaining, total)
    }
}
