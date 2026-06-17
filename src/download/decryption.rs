use aes::Aes128;
use cbc::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

type Aes128CbcDec = cbc::Decryptor<Aes128>;

const ENCRYPTED_MARKER: u64 = 0x0A_0A_0A_0A;

const AES_KEY: [u8; 16] = [
    91, 99, 219, 17, 59, 122, 243, 224, 177, 67, 85, 86, 200, 249, 83, 12,
];
const AES_IV: [u8; 16] = [
    113, 231, 4, 5, 53, 58, 119, 139, 250, 111, 188, 48, 50, 27, 149, 146,
];

#[derive(thiserror::Error, Debug)]
pub enum DecryptError {
    #[error("encrypted header size {0} is invalid")]
    BadHeaderSize(u64),
    #[error("encrypted data size {0} is invalid")]
    BadEncryptedSize(u64),
    #[error("unable to decrypt the encrypted tile")]
    Cipher,
    #[error("read error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn decrypt(encrypted: Vec<u8>) -> Result<Vec<u8>, DecryptError> {
    let mut c = Cursor::new(encrypted);

    let marker = read_u32_le(&mut c)?;
    if marker != ENCRYPTED_MARKER {
        return Ok(c.into_inner());
    }

    let end_position = c.seek(SeekFrom::End(-4))?;
    let header_size = read_u32_le(&mut c)?;
    if 4 + header_size > end_position {
        return Err(DecryptError::BadHeaderSize(header_size));
    }

    let mut decrypted = Vec::new();
    c.seek(SeekFrom::Start(4))?;
    c = read_size(c, &mut decrypted, header_size)?;

    let encrypted_size = read_u32_le(&mut c)?;
    if 4 + header_size + 4 + encrypted_size > end_position {
        return Err(DecryptError::BadEncryptedSize(encrypted_size));
    }
    let mut block = Vec::new();
    c = read_size(c, &mut block, encrypted_size)?;

    let decrypted_chunk = Aes128CbcDec::new(&AES_KEY.into(), &AES_IV.into())
        .decrypt_padded_mut::<NoPadding>(&mut block)
        .map_err(|_| DecryptError::Cipher)?;
    decrypted.write_all(decrypted_chunk)?;

    let footer_size = end_position - encrypted_size - 4 - header_size - 4;
    read_size(c, &mut decrypted, footer_size)?;

    Ok(decrypted)
}

fn read_u32_le<T: Read>(buf: &mut T) -> std::io::Result<u64> {
    let mut bytes = [0u8; 4];
    buf.read_exact(&mut bytes)?;
    Ok(u64::from(u32::from_le_bytes(bytes)))
}

fn read_size<T: Read>(c: T, dest: &mut Vec<u8>, size: u64) -> Result<T, std::io::Error> {
    let mut wrapper = c.take(size);
    wrapper.read_to_end(dest)?;
    Ok(wrapper.into_inner())
}
