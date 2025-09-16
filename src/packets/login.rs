use alloc::vec::Vec;
use embassy_net::tcp::TcpSocket;
use log::{info, warn};
use mcproto_rs::{
    protocol::State,
    types::CountedArray,
    v1_21_8::{LoginEncryptionRequestSpec, LoginSuccessSpec, Packet772},
};
use rsa::pkcs8::der::Encode as _;

use crate::{
    encryption::ServerEncryption,
    errors::MinecraftError,
    packets::{write_packet, PlayerContext, PlayerEncryptionContext, PlayerLoginContext, EMPTY_STRING},
};

pub async fn handle_login_packets(
    packet: Packet772,
    context: &mut PlayerContext,
    socket: &mut TcpSocket<'_>,
    encryption: &ServerEncryption<'static>,
) -> Result<(Option<Packet772>, bool), MinecraftError> {
    match packet {
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
            return Ok((None, true));
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

            // Encryption enabled now because of above
            let login_success = Packet772::LoginSuccess(LoginSuccessSpec {
                uuid: login_context.uuid,
                username: login_context.username.clone(),
                properties: CountedArray::from(Vec::new()),
            });

            write_packet(socket, context, login_success).await?;
            return Ok((None, true));
        }
        Packet772::LoginAcknowledged(_) => {
            if matches!(context.state, State::Login) {
                context.state = State::Configuration
            }
            return Ok((None, true));
        }
        _ => Ok((Some(packet), true)),
    }
}
