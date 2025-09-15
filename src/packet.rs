use core::mem;

use aes::cipher::AsyncStreamCipher;
use aes::cipher::BlockEncryptMut;
use aes::cipher::KeyIvInit;
use alloc::{borrow::ToOwned as _, string::String, vec::Vec};
use embassy_net::tcp::TcpSocket;
use log::{info, warn};
use mcproto_rs::{
    Serialize,
    protocol::{HasPacketBody, HasPacketId, State},
    status::{StatusPlayersSpec, StatusSpec, StatusVersionSpec},
    types::{Chat, CountedArray, VarInt},
    uuid::UUID4,
    v1_21_8::{
        HandshakeIntent, LoginEncryptionRequestSpec, LoginSuccessSpec, Packet772, PingResponseSpec,
        StatusResponseSpec,
    },
};
use rsa::pkcs8::der::Encode;

use crate::{
    encryption::ServerEncryption,
    errors::MinecraftError,
    utils::{SliceSerializer, text},
};

const PACKET_WRITE_BUFFER_SIZE: usize = 4096;
pub const VAR_INT_BUF_SIZE: usize = 5;
pub const EMPTY_STRING: String = String::new();

struct PlayerLoginContext {
    verify_token: Option<Vec<u8>>,
    uuid: UUID4,
    username: String,
}

type Aes128Cfb8Enc = cfb8::Encryptor<aes::Aes128>;
type Aes128Cfb8Dec = cfb8::Decryptor<aes::Aes128>;
struct PlayerEncryptionContext {
    encrypter: Aes128Cfb8Enc,
    decrypter: Aes128Cfb8Dec,
    shared_token: [u8; 16],
}

impl PlayerEncryptionContext {
    pub fn new(shared_token: Vec<u8>) -> Self {
        info!("token length {}", shared_token.len() * 8);
        let shared_token: [u8; 16] = shared_token
            .try_into()
            .expect("shared token not long enough");
        Self {
            encrypter: Aes128Cfb8Enc::new(&shared_token.into(), &shared_token.into()),
            decrypter: Aes128Cfb8Dec::new(&shared_token.into(), &shared_token.into()),
            shared_token,
        }
    }
}

pub struct PlayerContext {
    pub state: State,
    login_context: Option<PlayerLoginContext>,
    encryption_context: Option<PlayerEncryptionContext>,
}

impl Default for PlayerContext {
    fn default() -> Self {
        Self {
            state: State::Handshaking,
            login_context: None,
            encryption_context: None,
        }
    }
}

async fn write_encryption_transparent<const T: usize>(
    socket: &mut TcpSocket<'_>,
    context: &mut PlayerContext,
    slices: [&mut [u8]; T],
) -> Result<(), MinecraftError> {
    for slice in slices {
        if let Some(encryption) = &mut context.encryption_context {
            for chunk in slice.chunks_mut(1) {
                encryption.encrypter.encrypt_block_mut(chunk.into());
            }
        }

        let mut written = 0;

        while written < slice.len() {
            written += socket.write(&slice[written..]).await?;
        }
    }

    Ok(())
}

async fn write_packet(
    socket: &mut TcpSocket<'_>,
    context: &mut PlayerContext,
    packet: Packet772,
) -> Result<(), MinecraftError> {
    let mut serializer_backend = [0u8; PACKET_WRITE_BUFFER_SIZE];
    let mut serializer = SliceSerializer::create(&mut serializer_backend);
    packet.id().mc_serialize(&mut serializer)?;
    packet.mc_serialize_body(&mut serializer)?;

    let packet = serializer.finish();

    let mut length_serializer_backend = [0u8; VAR_INT_BUF_SIZE];
    let mut length_serializer = SliceSerializer::create(&mut length_serializer_backend);
    let packet_len = VarInt(packet.len().try_into().unwrap());
    packet_len
        .mc_serialize(&mut length_serializer)
        .expect("failed to serialize packet length");
    let packet_len_len = length_serializer.finish().len();

    write_encryption_transparent(
        socket,
        context,
        [&mut length_serializer_backend[..packet_len_len], packet],
    )
    .await?;

    Ok(())
}

pub async fn process_packet(
    packet: Packet772,
    context: &mut PlayerContext,
    socket: &mut TcpSocket<'_>,
    encryption: &ServerEncryption<'static>,
) -> Result<bool, MinecraftError> {
    // info!("{:?}", packet);
    match packet {
        Packet772::PingRequest(v) => {
            let response = Packet772::PingResponse(PingResponseSpec { payload: v.payload });

            write_packet(socket, context, response).await?;
            return Ok(true);
        }
        Packet772::Handshake(v) => {
            info!("trying to connect with version: {}", v.protocol_version);

            match v.intent {
                HandshakeIntent::Status => {
                    context.state = State::Status;
                }
                HandshakeIntent::Login => {
                    context.state = State::Login;
                }
                HandshakeIntent::Transfer => {
                    warn!("transfer not supported")
                }
            }
        }
        Packet772::StatusRequest(_) => {
            let response = Packet772::StatusResponse(StatusResponseSpec {
                response: StatusSpec {
                    version: Some(StatusVersionSpec {
                        name: "1.21.8".to_owned(),
                        protocol: 772,
                    }),
                    players: StatusPlayersSpec {
                        max: 10,
                        online: 0,
                        sample: Vec::new(),
                    },
                    description: Chat::Text(text(
                        "blockchain - instead of bitcoin, a block game for your key chain!",
                    )),
                    favicon: None,
                    enforces_secure_chat: false,
                },
            });

            write_packet(socket, context, response).await?;
            return Ok(true);
        }
        Packet772::LoginStart(spec) => {
            info!("{} is connecting...", spec.name);

            let spki = rsa::pkcs8::SubjectPublicKeyInfo::from_key(&encryption.public)?
                .to_der()
                .expect("failed to serialize to der");

            let random = encryption.random_data().await;

            let encryption_request =
                Packet772::LoginEncryptionRequest(LoginEncryptionRequestSpec {
                    server_id: EMPTY_STRING,
                    public_key: CountedArray::from(spki),
                    verify_token: CountedArray::from(random.clone()),
                    should_authenticate: false,
                });

            context.login_context = Some(PlayerLoginContext {
                verify_token: Some(random),
                uuid: spec.uuid,
                username: spec.name,
            });

            write_packet(socket, context, encryption_request).await?;
            return Ok(true);
        }
        Packet772::LoginEncryptionResponse(spec) => {
            let login_context = if let Some(login_context) = &mut context.login_context {
                login_context
            } else {
                return Err(MinecraftError::Unauthorized);
            };

            let verify_token = if let Some(verify_token) = login_context.verify_token.take() {
                verify_token
            } else {
                return Err(MinecraftError::Unauthorized);
            };
            let decrypted_verify = encryption.decrypt_data(&spec.verify_token).await?;

            if !decrypted_verify.eq(&verify_token) {
                return Err(MinecraftError::Unauthorized);
            }

            let decrypted_secret = encryption.decrypt_data(&spec.shared_secret).await?;
            context.encryption_context = Some(PlayerEncryptionContext::new(decrypted_secret));
            info!(
                "shared token length: {}",
                mem::size_of::<PlayerEncryptionContext>()
            );

            // Encryption enabled now because of above
            let login_success = Packet772::LoginSuccess(LoginSuccessSpec {
                uuid: login_context.uuid,
                username: login_context.username.clone(),
                properties: CountedArray::from(Vec::new()),
            });

            write_packet(socket, context, login_success).await?;
            return Ok(true);
        }
        _ => {
            info!("no handler for type {:?}", packet.id());
        }
    };

    Ok(true)
}
