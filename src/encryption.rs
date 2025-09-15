use alloc::{boxed::Box, vec::Vec};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use esp_hal::{Async, rng::Rng, rsa::Rsa};
use rsa::{
    Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey,
    rand_core::{CryptoRng, RngCore},
};

pub struct ServerEncryption<'a> {
    rsa: Rsa<'a, Async>,
    hardware: Mutex<NoopRawMutex, HardwareWrapper>,
    pub private: RsaPrivateKey,
    pub public: RsaPublicKey,
}

struct HardwareWrapper {
    rng: Rng,
}

impl RngCore for HardwareWrapper {
    fn next_u32(&mut self) -> u32 {
        self.rng.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        self.rng.read(dst);
    }
}

impl CryptoRng for HardwareWrapper {}

impl<'a> ServerEncryption<'a> {
    pub fn new(rsa_peripheral: Rsa<'a, Async>, rng: Rng) -> Self {
        let mut hardware_wrapper = HardwareWrapper { rng };
        let private = RsaPrivateKey::new(&mut hardware_wrapper, 1024)
            .expect("failed to generate private key");
        let public = private.to_public_key();
        Self {
            hardware: Mutex::new(hardware_wrapper),
            rsa: rsa_peripheral,
            private,
            public,
        }
    }

    pub async fn encrypt_data(&self, data: &[u8]) -> Result<Vec<u8>, rsa::Error> {
        let mut hardware_rng = self.hardware.lock().await;
        let enc_data = self
            .public
            .encrypt(&mut hardware_rng, Pkcs1v15Encrypt, data)?;

        Ok(enc_data)
    }

    pub async fn decrypt_data(&self, data: &[u8]) -> Result<Vec<u8>, rsa::Error > {
        let dec_data = self.private.decrypt(Pkcs1v15Encrypt, data)?;
        Ok(dec_data)
    }

    pub async fn random_data(&self) -> Vec<u8> {
        let mut hardware_rng = self.hardware.lock().await;
        let mut random_buffer = [0u8; 64];
        hardware_rng.rng.read(&mut random_buffer);
        random_buffer.to_vec()
    }
}
