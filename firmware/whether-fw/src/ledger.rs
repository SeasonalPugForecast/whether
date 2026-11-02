//! The One Call call ledger on the device: the reservation is kept in
//! the `nvs` partition through sequential-storage, which spreads writes over the
//! partition's sectors. The Wi-Fi driver runs with NVS disabled, so the
//! partition is ours.

use embassy_embedded_hal::adapter::BlockingAsync;
use esp_bootloader_esp_idf::partitions::{
    self, DataPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionType,
};
use esp_hal::peripherals::FLASH;
use esp_storage::{FlashStorage, FlashStorageError};
use log::warn;
use sequential_storage::cache::{Cache, Uncached};
use sequential_storage::map::{MapConfig, MapStorage, SerializationError, Value};
use whether_core::model::ProviderId;
use whether_providers::Reservation;

/// Calls reserved per flash write.
pub const BLOCK: u32 = 10;

type Flash = BlockingAsync<FlashStorage<'static>>;
type Map = MapStorage<u8, Flash, Cache<Uncached, Uncached, Uncached, u8>>;

/// Why the ledger cannot be used; metered calls stay off.
#[derive(Debug)]
pub enum LedgerError {
    /// The partition table could not be read.
    PartitionTable(partitions::Error),
    /// There is no `nvs` partition.
    NoPartition,
    /// The partition does not suit sequential-storage.
    Config,
    /// Reading or writing the flash failed.
    Storage(sequential_storage::Error<FlashStorageError>),
}

/// A reservation as stored: the UTC day and the count, little-endian.
struct Stored(Reservation);

impl Value<'_> for Stored {
    fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize, SerializationError> {
        let out = buffer
            .get_mut(..12)
            .ok_or(SerializationError::BufferTooSmall)?;
        out[..8].copy_from_slice(&self.0.day.to_le_bytes());
        out[8..].copy_from_slice(&self.0.reserved.to_le_bytes());
        Ok(12)
    }

    fn deserialize_from(buffer: &[u8]) -> Result<(Self, usize), SerializationError> {
        let day = buffer
            .get(..8)
            .and_then(|b| b.try_into().ok())
            .ok_or(SerializationError::InvalidFormat)?;
        let reserved = buffer
            .get(8..12)
            .and_then(|b| b.try_into().ok())
            .ok_or(SerializationError::InvalidFormat)?;
        Ok((
            Self(Reservation {
                day: i64::from_le_bytes(day),
                reserved: u32::from_le_bytes(reserved),
            }),
            12,
        ))
    }
}

/// The key a provider's reservation is stored under.
fn key(provider: ProviderId) -> u8 {
    match provider {
        ProviderId::OpenMeteo => 1,
        ProviderId::MetNorway => 2,
        ProviderId::OpenWeather => 3,
        ProviderId::Nws => 4,
        ProviderId::Acis => 5,
        // Host-only and monthly: the device reserves day budgets only.
        ProviderId::PirateWeather => 6,
    }
}

/// The reservations in flash.
pub struct LedgerStore {
    map: Map,
    buffer: [u8; 32],
}

impl core::fmt::Debug for LedgerStore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LedgerStore").finish_non_exhaustive()
    }
}

impl LedgerStore {
    /// Opens the store in the `nvs` partition.
    pub fn open(flash: FLASH<'static>) -> Result<Self, LedgerError> {
        let mut storage = FlashStorage::new(flash);
        let mut table = [0u8; PARTITION_TABLE_MAX_LEN];
        let nvs = partitions::read_partition_table(&mut storage, &mut table)
            .and_then(|pt| pt.find_partition(PartitionType::Data(DataPartitionSubType::Nvs)))
            .map_err(LedgerError::PartitionTable)?
            .ok_or(LedgerError::NoPartition)?;
        let range = nvs.offset()..nvs.offset() + nvs.len();
        let config = MapConfig::try_new(range).map_err(|_| LedgerError::Config)?;
        Ok(Self {
            map: MapStorage::new(BlockingAsync::new(storage), config, Cache::new_uncached()),
            buffer: [0; 32],
        })
    }

    /// The stored reservation for `provider`. A partition holding something else
    /// (e.g. another firmware's NVS data) is erased first.
    pub async fn load(&mut self, provider: ProviderId) -> Result<Option<Reservation>, LedgerError> {
        match self
            .map
            .fetch_item::<Stored>(&mut self.buffer, &key(provider))
            .await
        {
            Ok(stored) => Ok(stored.map(|s| s.0)),
            Err(sequential_storage::Error::Corrupted { .. }) => {
                warn!("the nvs partition holds foreign data; erasing it");
                self.map.erase_all().await.map_err(LedgerError::Storage)?;
                Ok(None)
            }
            Err(e) => Err(LedgerError::Storage(e)),
        }
    }

    /// Stores `reservation` for `provider`.
    pub async fn save(
        &mut self,
        provider: ProviderId,
        reservation: Reservation,
    ) -> Result<(), LedgerError> {
        self.map
            .store_item(&mut self.buffer, &key(provider), &Stored(reservation))
            .await
            .map_err(LedgerError::Storage)
    }
}
