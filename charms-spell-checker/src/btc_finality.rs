use std::io::Cursor;

use bitcoin::{consensus::Decodable, Block, MerkleBlock, Txid};
use charms_client::BitcoinFinalityInput;
use charms_data::util;

fn verify_block_inclusion(input: BitcoinFinalityInput) -> bool {
    let mut block_cursor = Cursor::new(input.block_bytes);
    let block: Block = Block::consensus_decode(&mut block_cursor).expect("Failed to decode block");

    let proof_bytes = sp1_zkvm::io::read::<Vec<u8>>();
    let mut proof_cursor = std::io::Cursor::new(&proof_bytes);

    // 2. Parse partial Merkle tree
    let merkle_block: MerkleBlock =
        MerkleBlock::consensus_decode(&mut proof_cursor).expect("Failed to parse MerkleBlock");

    let pmt = merkle_block.txn;
    println!("PartialMerkleTree Decoded!");

    // 3. Extract matched txids and verify Merkle root
    let mut matched_txids: Vec<Txid> = Vec::new();
    let mut indexes: Vec<u32> = Vec::new();

    let merkle_root = pmt
        .extract_matches(&mut matched_txids, &mut indexes)
        .expect("Failed to extract matches");

    println!("Extract root");

    println!(
        "Block Merkle root (from header): {}",
        block.header.merkle_root
    );
    println!("Merkle root (from proof):       {}", merkle_root);

    assert_eq!(
        block.header.merkle_root, merkle_root,
        "Merkle root does not match block header!"
    );

    let txid_hash = input
        .expected_tx
        .to_string()
        .parse()
        .expect("Couldn't deserialize hash");
    let expected_txid: Txid = Txid::from_raw_hash(txid_hash);

    if matched_txids.contains(&expected_txid) {
        println!("✅ Proof confirms inclusion of txid {}", expected_txid);
        return true;
    } else {
        println!("❌ Proof does NOT include the expected txid!");
        return false;
    };
}

pub fn verify_block_inclusion_main() {
    // Read an input to the program.
    let input_vec = sp1_zkvm::io::read_vec();
    let inclusion_input: BitcoinFinalityInput = util::read(input_vec.as_slice()).unwrap();

    assert!(verify_block_inclusion(inclusion_input));
}

#[cfg(test)]
mod test {

    use bitcoin::{
        consensus::Decodable,
        hashes::{hex::FromHex, sha256d::Hash},
        Block, MerkleBlock, Txid,
    };
    use std::{fs, io::Cursor};

    /// Get bitcoin block by running
    /// `bitcoin-cli getblock 00000000000000000366c11e195318fd96ccce10b527c46560f9aa325f9e4bee false
    /// > block_hex.txt`
    #[test]
    fn test_tx_proof() {
        let hex_string = fs::read_to_string("block_hex.txt")
            .expect("Failed to read block_head.txt")
            .trim()
            .to_string(); // Remove whitespace/newlines if any

        let block_bytes = Vec::from_hex(&hex_string).expect("Invalid hex in block_head.txt");

        let mut block_cursor = Cursor::new(block_bytes);

        let block: Block =
            Block::consensus_decode(&mut block_cursor).expect("Failed to decode block!");

        println!("Proof");

        let proof_hex = "0200000078e880f1c1e2a539a92db2bcbb987117dc3259980581dd280000000000000000265708246b58f7a6ed29ffd4a5f8c2767dc76449c56769362d203e796b22e4c4af31fa53581c2e1889482ae14e0700000c22d386b85514314d0c7f7960a6358471357dd0caa3a6ca9f0d83a3e6921f2bedcd8e2fed111de27b13d4b313280448d3c7195fe5bfa8df6bb650cd3c904078c638af94d2c5a103569efd46b70d04b684d43166686e18eff457f74634548008efe1cb4aefc683ea2e8731fa088d2f278645f22917d3379f7f28ff6b81959c145abf454ae6c15b5e1dfa1cb83058fb29c7dd01fe6a6a121f503f187b2a2682e77e3d03edcb3d1ebfbc793d722f88e7938e7288cfeea40c82660a837fb49b6c98cc5a575f58e8bd9c5c75929d1aecafbc146ccf703d79b015308f8fc8257733d6532053f1040e63731ae05a5f1c5a521b1339db0c3812651d3c7d10a32130a28c30c99a978707c8df8563a547a211bd72c7cb592a5c423f4d7c200a586fbc0d118ece6610a14c6c7a42c990c1b00c1d43ee41b4c3f1c85a1715d7bfaa008605eb97638fbf28150fe8ea96fbb0fd54e4686253c9f14c59e9fb6b1696131e8d17b518aa8590baf06e339cc6da4f35378a5ec7c23fa298ddf4daf572b3c094c90144f503ff0f00";

        let proof_bytes = Vec::from_hex(proof_hex).expect("Invalid hex");
        let mut proof_cursor = std::io::Cursor::new(&proof_bytes);

        // 2. Parse partial Merkle tree
        let merkle_block: MerkleBlock =
            MerkleBlock::consensus_decode(&mut proof_cursor).expect("Failed to parse MerkleBlock");

        let pmt = merkle_block.txn;

        println!("PMT Decode");

        // 3. Extract matched txids and verify Merkle root
        let mut matched_txids: Vec<Txid> = Vec::new();
        let mut indexes: Vec<u32> = Vec::new();

        let merkle_root = pmt
            .extract_matches(&mut matched_txids, &mut indexes)
            .expect("Failed to extract matches");

        println!("Extract root");

        println!(
            "Block Merkle root (from header): {}",
            block.header.merkle_root
        );
        println!("Merkle root (from proof):       {}", merkle_root);

        assert_eq!(
            block.header.merkle_root, merkle_root,
            "Merkle root does not match block header!"
        );

        let txid_hex = "ed2b1f92e6a3830d9fcaa6a3cad07d35718435a660797f0c4d311455b886d322";
        let txid_hash: Hash = txid_hex.parse().expect("Couldn't deserialize hash");
        let expected_txid: Txid = Txid::from_raw_hash(txid_hash);

        if matched_txids.contains(&expected_txid) {
            println!("✅ Proof confirms inclusion of txid {}", expected_txid);
        } else {
            println!("❌ Proof does NOT include the expected txid!");
        }
    }
}
