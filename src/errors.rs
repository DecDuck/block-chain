use mcproto_rs::{DeserializeErr, SerializeErr};

#[derive(Debug)]
pub enum MinecraftError {
    ConnectionError(embassy_net::tcp::Error),
    SerializeError(SerializeErr),
    DeserializeError(DeserializeErr),
    EncryptionError(rsa::Error),
    CertificateParsingError(rsa::pkcs8::spki::Error),
    InvalidPacketHeader,
    Unauthorized,
}

impl From<embassy_net::tcp::Error> for MinecraftError {
    fn from(value: embassy_net::tcp::Error) -> Self {
        MinecraftError::ConnectionError(value)
    }
}

impl From<SerializeErr> for MinecraftError {
    fn from(value: SerializeErr) -> Self {
        MinecraftError::SerializeError(value)
    }
}

impl From<rsa::Error> for MinecraftError {
    fn from(value: rsa::Error) -> Self {
        MinecraftError::EncryptionError(value)
    }
}

impl From<rsa::pkcs8::spki::Error> for MinecraftError {
    fn from(value: rsa::pkcs8::spki::Error) -> Self {
        MinecraftError::CertificateParsingError(value)
    }
}

impl From<DeserializeErr> for MinecraftError {
    fn from(value: DeserializeErr) -> Self {
        MinecraftError::DeserializeError(value)
    }
}