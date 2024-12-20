#![no_std]
#![feature(auto_traits, negative_impls)]

use anyhow::{anyhow, Error, Result};
use ark_std::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    format, vec,
    vec::Vec,
};
use core::{convert::TryInto, fmt};
use serde::{
    de,
    de::{DeserializeOwned, SeqAccess, Visitor},
    ser::SerializeTuple,
    Deserialize, Deserializer, Serialize, Serializer,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    /// Input UTXOs: **must** have id set.
    pub ins: Vec<Utxo>,
    /// Reference UTXOs: **must** have id set.
    pub refs: Vec<Utxo>,
    /// Transaction outputs: **must not** have id set.
    pub outs: Vec<Utxo>,
}

impl Transaction {
    pub fn pre_req_txids(&self) -> BTreeSet<TxId> {
        let mut txids = BTreeSet::new();
        for utxo in self.ins.iter().chain(self.refs.iter()) {
            if let Some(id) = utxo.id.clone() {
                txids.insert(id.0);
            }
        }
        txids
    }

    pub fn app_ids(&self) -> BTreeSet<AppId> {
        let mut app_ids = BTreeSet::new();
        for utxo in self.ins.iter().chain(self.outs.iter()) {
            for app_id in utxo.charm.keys() {
                app_ids.insert(app_id.clone());
            }
        }
        app_ids
    }
}

/// Charm is essentially an app-level UTXO that can carry tokens, NFTs, arbitrary app state.
/// Structurally it is a sorted map of `app_id -> app_state`
pub type Charm = BTreeMap<AppId, AppState>;

pub type Witness = BTreeMap<AppId, WitnessData>;

pub type VKs = BTreeMap<VkHash, VK>;

// UTXO as presented to the validation predicate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Utxo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<UtxoId>,
    pub charm: Charm,
}

impl Utxo {
    #[inline]
    pub fn get(&self, key: &AppId) -> Option<&AppState> {
        self.charm.get(key)
    }
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct UtxoId(pub TxId, pub u32);

impl UtxoId {
    pub fn to_bytes(&self) -> [u8; 36] {
        let mut bytes = [0u8; 36];
        bytes[..32].copy_from_slice(&self.0); // Copy TxId
        bytes[32..].copy_from_slice(&self.1.to_le_bytes()); // Copy index as little-endian
        bytes
    }

    pub fn from_bytes(bytes: [u8; 36]) -> Self {
        let mut txid = [0u8; 32];
        txid.copy_from_slice(&bytes[..32]);
        let index = u32::from_le_bytes(bytes[32..].try_into().unwrap());
        UtxoId(txid, index)
    }

    pub fn from_str(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err(anyhow!("expected format: txid_hex:index"));
        }

        let txid_bytes =
            hex::decode(parts[0]).map_err(|e| anyhow!("invalid txid string: {:?}", e))?;
        let txid = txid_bytes
            .try_into()
            .map_err(|e| anyhow!("invalid txid bytes: {:?}", e))?;

        let index = parts[1]
            .parse::<u32>()
            .map_err(|e| anyhow!("invalid index: {}", e))?;

        Ok(UtxoId(txid, index))
    }
}

impl Serialize for UtxoId {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&format!("{}:{}", hex::encode(self.0), self.1))
        } else {
            serializer.serialize_bytes(self.to_bytes().as_ref())
        }
    }
}

impl<'de> Deserialize<'de> for UtxoId {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UtxoIdVisitor;

        impl<'de> Visitor<'de> for UtxoIdVisitor {
            type Value = UtxoId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string in format 'txid_hex:index' or a tuple (TxId, u32)")
            }

            // Handle human-readable format ("txid_hex:index")
            fn visit_str<E>(self, value: &str) -> Result<UtxoId, E>
            where
                E: de::Error,
            {
                // Split at ':'
                let parts: Vec<&str> = value.split(':').collect();
                if parts.len() != 2 {
                    return Err(E::custom("expected format: txid_hex:index"));
                }

                // Decode txid hex
                let txid_bytes = hex::decode(parts[0])
                    .map_err(|e| E::custom(format!("invalid txid hex: {}", e)))?;

                // Convert tx_bytes into TxId array
                let txid = txid_bytes
                    .try_into()
                    .map_err(|e| E::custom(format!("invalid txid bytes: {:?}", e)))?;

                // Parse index
                let index = parts[1]
                    .parse::<u32>()
                    .map_err(|e| E::custom(format!("invalid index: {}", e)))?;

                Ok(UtxoId(txid, index))
            }

            // Handle non-human-readable byte format [u8; 36]
            fn visit_bytes<E>(self, v: &[u8]) -> core::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(UtxoId::from_bytes(v.try_into().map_err(|e| {
                    E::custom(format!("invalid utxo_id bytes: {}", e))
                })?))
            }
        }

        if deserializer.is_human_readable() {
            deserializer.deserialize_str(UtxoIdVisitor)
        } else {
            deserializer.deserialize_bytes(UtxoIdVisitor)
        }
    }
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct AppId {
    pub tag: char,
    pub id: UtxoId,
    pub vk_hash: VkHash,
}

impl Serialize for AppId {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let tag = self.tag;
            let id = format!("{}:{}", hex::encode(self.id.0), self.id.1);
            let vk_hash = hex::encode(&self.vk_hash.0);
            serializer.serialize_str(&format!("{}/{}/{}", tag, id, vk_hash))
        } else {
            let mut s = serializer.serialize_tuple(3)?;
            s.serialize_element(&self.tag)?;
            s.serialize_element(&self.id)?;
            s.serialize_element(&self.vk_hash)?;
            s.end()
        }
    }
}

impl<'de> Deserialize<'de> for AppId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AppIdVisitor;

        impl<'de> Visitor<'de> for AppIdVisitor {
            type Value = AppId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string in format 'tag_char/txid_hex:index_int/vk_hash_hex' or a struct with tag, utxo_id and vk_hash fields")
            }

            // Handle human-readable format ("tag_hex/vk_hash_hex")
            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                // Split the string at '/'
                let parts: Vec<&str> = value.split('/').collect();
                if parts.len() != 3 {
                    return Err(E::custom(
                        "expected format: tag_char/txid_hex:index_int/vk_hash_hex",
                    ));
                }

                // Decode the hex strings
                let tag: char = {
                    let mut chars = parts[0].chars();
                    let Some(tag) = chars.next() else {
                        return Err(E::custom("expected tag"));
                    };
                    let None = chars.next() else {
                        return Err(E::custom("tag must be a single character"));
                    };
                    tag
                };

                let id = {
                    let utxo_id_parts: Vec<&str> = parts[1].split(':').collect();
                    if utxo_id_parts.len() != 2 {
                        return Err(E::custom("expected utxo_id format: txid_hex:index"));
                    }
                    let txid_bytes = hex::decode(utxo_id_parts[0])
                        .map_err(|e| E::custom(format!("invalid txid hex: {}", e)))?;

                    let txid = txid_bytes
                        .try_into()
                        .map_err(|e| E::custom(format!("invalid txid bytes: {:?}", e)))?;

                    let index = utxo_id_parts[1]
                        .parse::<u32>()
                        .map_err(|e| E::custom(format!("invalid index: {}", e)))?;

                    UtxoId(txid, index)
                };

                let vk_hash_bytes = hex::decode(parts[2])
                    .map_err(|e| E::custom(format!("invalid vk_hash hex: {}", e)))?;

                // Convert vk_hash bytes to VkHash
                let vk_hash = VkHash(
                    vk_hash_bytes
                        .try_into()
                        .map_err(|e| E::custom(format!("invalid vk_hash: {:?}", e)))?,
                );

                Ok(AppId { tag, id, vk_hash })
            }

            fn visit_seq<A>(self, mut seq: A) -> core::result::Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let tag = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("tag"))?;
                let id = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("id"))?;
                let vk_hash = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::missing_field("vk_hash"))?;

                Ok(AppId { tag, id, vk_hash })
            }
        }

        if deserializer.is_human_readable() {
            deserializer.deserialize_str(AppIdVisitor)
        } else {
            deserializer.deserialize_tuple(3, AppIdVisitor)
        }
    }
}

pub type AppState = Data;

pub type TxId = [u8; 32];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WitnessData {
    pub proof: Data,
    pub public_input: Data,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct VkHash(pub [u8; 32]);

pub type VK = Vec<u8>;

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Data(Box<[u8]>);

impl Data {
    pub fn empty() -> Self {
        Self(Box::new([]))
    }

    pub fn try_into<T: DeserializeOwned>(&self) -> Result<T> {
        ciborium::de::from_reader(self.0.as_ref())
            .map_err(|e| anyhow!("failed to convert from Data: {}", e))
    }
}

auto trait NotData {}
impl !NotData for Data {}

impl<T> From<T> for Data
where
    T: Serialize + NotData,
{
    fn from(value: T) -> Self {
        let mut data = vec![];
        ciborium::ser::into_writer(&value, &mut data).unwrap();
        Self(data.into_boxed_slice())
    }
}

impl TryFrom<&Data> for u64 {
    type Error = Error;

    fn try_from(data: &Data) -> Result<Self> {
        data.try_into()
    }
}

pub const TOKEN: char = 't';
pub const NFT: char = 'n';

pub fn token_amounts_balanced(app_id: &AppId, tx: &Transaction) -> bool {
    match (
        sum_token_amount(app_id, &tx.ins),
        sum_token_amount(app_id, &tx.outs),
    ) {
        (Ok(amount_in), Ok(amount_out)) => amount_in == amount_out,
        (..) => false,
    }
}

pub fn nft_state_preserved(app_id: &AppId, tx: &Transaction) -> bool {
    let nft_states_in = app_state_multiset(app_id, &tx.ins);
    let nft_states_out = app_state_multiset(app_id, &tx.outs);

    nft_states_in == nft_states_out
}

pub fn app_state_multiset<'a>(
    app_id: &AppId,
    utxos: &'a Vec<Utxo>,
) -> BTreeMap<&'a AppState, usize> {
    utxos
        .iter()
        .filter_map(|utxo| utxo.get(app_id))
        .fold(BTreeMap::new(), |mut r, s| {
            match r.get_mut(&s) {
                Some(count) => *count += 1,
                None => {
                    r.insert(s, 1);
                }
            }
            r
        })
}

pub fn sum_token_amount(self_app_id: &AppId, utxos: &[Utxo]) -> Result<u64> {
    let mut in_amount: u64 = 0;
    for utxo in utxos {
        // We only care about UTXOs that have our token.
        if let Some(state) = utxo.get(self_app_id) {
            let utxo_amount: u64 = state.try_into()?;
            in_amount += utxo_amount;
        }
    }
    Ok(in_amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn zk_meme_token_policy(app_id: &AppId, tx: &Transaction, x: &Data, w: &Data) {
        assert_eq!(app_id.tag, TOKEN);

        // is_meme_token_creator is a function that checks that
        // the spender is the creator of this meme token.
        // In our policy, the token creator can mint and burn tokens at will.
        assert!(token_amounts_balanced(&app_id, &tx) || is_meme_token_creator(x, w));
    }

    fn is_meme_token_creator(_x: &Data, _w: &Data) -> bool {
        // TODO check the signature in the witness
        false
    }

    #[test]
    fn test_zk_meme_token_validator() {
        let token_app_id = AppId {
            tag: TOKEN,
            id: Default::default(),
            vk_hash: Default::default(),
        };

        let ins = vec![Utxo {
            id: Some(UtxoId::default()),
            charm: Charm::from([(token_app_id.clone(), 1u64.into())]),
        }];
        let outs = vec![Utxo {
            id: None,
            charm: Charm::from([(token_app_id.clone(), 1u64.into())]),
        }];

        let tx = Transaction {
            ins,
            refs: vec![],
            outs,
        };

        let empty = Data::empty();
        zk_meme_token_policy(&token_app_id, &tx, &empty, &empty); // pass if no panic
    }
}
