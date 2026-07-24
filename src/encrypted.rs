#![allow(dead_code)]

use std::sync::OnceLock;
use parking_lot::RwLock;

static ENCRYPTION_KEY: OnceLock<RwLock<[u8; 32]>> = OnceLock::new();

fn key() -> &'static RwLock<[u8; 32]> {
    ENCRYPTION_KEY.get_or_init(|| {
        let mut key = [0u8; 32];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut key);
        RwLock::new(key)
    })
}

pub fn encrypt(data: &[u8]) -> Vec<u8> {
    let guard = key().read();
    data.iter()
        .enumerate()
        .map(|(i, b)| b ^ guard[i % guard.len()])
        .collect()
}

pub fn decrypt(data: &[u8]) -> Vec<u8> {
    encrypt(data)
}

pub struct EncryptedString {
    ciphertext: Vec<u8>,
}

impl EncryptedString {
    pub fn new(plaintext: &str) -> Self {
        Self { ciphertext: encrypt(plaintext.as_bytes()) }
    }

    pub fn decrypt(&self) -> String {
        String::from_utf8(decrypt(&self.ciphertext)).unwrap_or_default()
    }
}

pub fn obfuscate(byte: u8, index: usize) -> u8 {
    byte ^ (0xAB + index as u8).wrapping_mul(0x37)
}
