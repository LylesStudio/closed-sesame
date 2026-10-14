use crate::container::{ContainerHeader, ContainerReader};
use crate::crypto::{KdfAlgorithm, Salt};
use crate::error::{Result, SesameError};
use crate::keymgmt::KeyEncryptingKey;
use std::path::Path;

#[derive(Clone, Copy)]
pub enum Credential<'a> {
    Password(&'a str),
    NoPassword,
}

pub fn read_header(path: &Path) -> Result<ContainerHeader> {
    ContainerReader::read_header(path)
}

pub fn prepare_credentials(path: &Path, credential: Credential<'_>) -> Result<KeyEncryptingKey> {
    let header = read_header(path)?;
    let salt = Salt::from_slice(&header.salt)?;
    match credential {
        Credential::Password("") => {
            Err(SesameError::InvalidInput("password cannot be empty".into()))
        }
        Credential::Password(_) if header.kdf_config.algorithm != KdfAlgorithm::Argon2id => Err(
            SesameError::InvalidInput("container uses no-password mode; pass --no-password".into()),
        ),
        Credential::Password(password) => {
            KeyEncryptingKey::from_password(password, &salt, header.kdf_config)
        }
        Credential::NoPassword if header.kdf_config.algorithm != KdfAlgorithm::NoPassword => Err(
            SesameError::InvalidInput("container requires a password".into()),
        ),
        Credential::NoPassword => KeyEncryptingKey::no_password(&salt, header.kdf_config),
    }
}
