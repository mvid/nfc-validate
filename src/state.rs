use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, StdResult, Storage};
use cosmwasm_storage::{ReadonlySingleton, singleton, Singleton, singleton_read};
use serde::de::DeserializeOwned;

use secret_toolkit::{
    serialization::{Bincode2, Json, Serde},
};

pub static CONFIG_KEY: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct State {
    pub admin: CanonicalAddr,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, JsonSchema)]
pub struct Tag {
    pub id: [u8; 7],
    pub change_key: Key,
    pub mac_read_key: Key,
    pub count: [u8; 3],
}

impl Tag {
    pub fn count(&self) -> u32 {
        return u8_3_to_u32(self.count);
    }

    fn uid(&self) -> u64 {
        return u8_7_to_u64(self.id)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, JsonSchema)]
pub struct Key {
    pub value: [u8; 16],
    pub version: u8,
}

pub const PREFIX_TAGS: &[u8] = b"tags";

pub fn config(storage: &mut dyn Storage) -> Singleton<State> {
    singleton(storage, CONFIG_KEY)
}

pub fn config_read(storage: &dyn Storage) -> ReadonlySingleton<State> {
    singleton_read(storage, CONFIG_KEY)
}

/// Returns StdResult<()> resulting from saving an item to storage
///
/// # Arguments
///
/// * `storage` - a mutable reference to the storage this item should go to
/// * `key` - a byte slice representing the key to access the stored item
/// * `value` - a reference to the item to store
pub fn save<T: Serialize, S: Storage>(storage: &mut S, key: &[u8], value: &T) -> StdResult<()> {
    storage.set(key, &Bincode2::serialize(value)?);
    Ok(())
}

/// Returns StdResult<Option<T>> from retrieving the item with the specified key.
/// Returns Ok(None) if there is no item with that key
///
/// # Arguments
///
/// * `storage` - a reference to the storage this item is in
/// * `key` - a byte slice representing the key that accesses the stored item
pub fn may_load<T: DeserializeOwned, S: Storage>(
    storage: &S,
    key: &[u8],
) -> StdResult<Option<T>> {
    match storage.get(key) {
        Some(value) => Bincode2::deserialize(&value).map(Some),
        None => Ok(None),
    }
}

/// Removes an item from storage
///
/// # Arguments
///
/// * `storage` - a mutable reference to the storage this item is in
/// * `key` - a byte slice representing the key that accesses the stored item
pub fn remove<S: Storage>(storage: &mut S, key: &[u8]) {
    storage.remove(key);
}

pub fn u32_to_u8_3(input: u32) -> [u8; 3] {
    assert!(input < 16_777_216);

    let mut output: [u8; 3] = [0; 3];
    let input_bytes = input.to_be_bytes();
    for i in 0..=2 {
        output[i] = input_bytes[i + 1];
    }

    return output;
}

pub fn u8_3_to_u32(input: [u8; 3]) -> u32 {
    let u32_bytes: [u8; 4] = [0x00, input[0], input[1], input[2]];

    return u32::from_be_bytes(u32_bytes);
}

pub fn u64_to_u8_7(input: u64) -> [u8; 7] {
    assert!(input < (2^(7*8) - 1));

    let mut output: [u8; 7] = [0; 7];
    let input_bytes = input.to_be_bytes();
    for i in 0..=6 {
        output[i] = input_bytes[i + 1];
    }

    return output;
}

pub fn u8_7_to_u64(input: [u8; 7]) -> u64 {
    let u64_bytes: [u8; 8] = [
        0x00,
        input[0],
        input[1],
        input[2],
        input[3],
        input[4],
        input[5],
        input[6],
    ];

    return u64::from_be_bytes(u64_bytes);
}
