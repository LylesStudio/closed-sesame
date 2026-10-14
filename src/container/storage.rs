//! Encrypted block and page storage primitives.

use crate::container::format::{
    BLOCK_SIZE, HEADER_SIZE, NONCE_SIZE, PAGE_PLAINTEXT, PAGE_SIZE, PAGES_PER_BLOCK, TAG_SIZE,
    page_nonce,
};
use crate::crypto::{self, Key};
use crate::error::{Result, SesameError};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

pub(crate) const FIRST_DATA_BLOCK: u64 = 2;

pub(crate) fn block_offset(block_idx: u64) -> u64 {
    HEADER_SIZE as u64 * 2 + block_idx * BLOCK_SIZE as u64
}

pub(crate) fn page_offset(page_idx: u64) -> u64 {
    let block = FIRST_DATA_BLOCK + page_idx / PAGES_PER_BLOCK as u64;
    let page = page_idx % PAGES_PER_BLOCK as u64;
    block_offset(block) + page * PAGE_SIZE as u64
}

pub(crate) fn write_full_block(
    file: &mut File,
    offset: u64,
    key: &Key,
    plaintext: &[u8],
) -> Result<()> {
    let max_plaintext = BLOCK_SIZE - NONCE_SIZE - TAG_SIZE;
    if plaintext.len() > max_plaintext {
        return Err(SesameError::Serialization(format!(
            "metadata requires {} bytes, block capacity is {max_plaintext} bytes",
            plaintext.len()
        )));
    }
    let mut padded = vec![0; max_plaintext];
    padded[..plaintext.len()].copy_from_slice(plaintext);
    let (encrypted, nonce) = crypto::encrypt(key, &padded)?;
    let mut block = vec![0; BLOCK_SIZE];
    block[..NONCE_SIZE].copy_from_slice(&nonce);
    block[NONCE_SIZE..NONCE_SIZE + encrypted.len()].copy_from_slice(&encrypted);
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&block)?;
    Ok(())
}

pub(crate) fn read_full_block(file: &mut File, offset: u64, key: &Key) -> Result<Vec<u8>> {
    let mut block = vec![0; BLOCK_SIZE];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut block)?;
    let mut nonce = [0u8; NONCE_SIZE];
    nonce.copy_from_slice(&block[..NONCE_SIZE]);
    crypto::decrypt(key, &block[NONCE_SIZE..], &nonce)
        .map_err(|error| SesameError::Corruption(format!("encrypted metadata block: {error}")))
}

pub(crate) fn write_page(
    file: &mut File,
    key: &Key,
    page_idx: u64,
    plaintext: &[u8],
) -> Result<()> {
    let nonce = page_nonce(
        page_idx / PAGES_PER_BLOCK as u64,
        page_idx % PAGES_PER_BLOCK as u64,
    );
    let mut padded = vec![0; PAGE_PLAINTEXT];
    let len = plaintext.len().min(PAGE_PLAINTEXT);
    padded[..len].copy_from_slice(&plaintext[..len]);
    let encrypted = crypto::encrypt_with_nonce(key, &padded, &nonce)?;
    let mut page = vec![0; PAGE_SIZE];
    page[..NONCE_SIZE].copy_from_slice(&nonce);
    page[NONCE_SIZE..NONCE_SIZE + encrypted.len()].copy_from_slice(&encrypted);
    file.seek(SeekFrom::Start(page_offset(page_idx)))?;
    file.write_all(&page)?;
    Ok(())
}

pub(crate) fn read_page(file: &mut File, key: &Key, page_idx: u64) -> Result<Vec<u8>> {
    let mut page = vec![0; PAGE_SIZE];
    file.seek(SeekFrom::Start(page_offset(page_idx)))?;
    file.read_exact(&mut page)?;
    let mut nonce = [0u8; NONCE_SIZE];
    nonce.copy_from_slice(&page[..NONCE_SIZE]);
    crypto::decrypt(key, &page[NONCE_SIZE..], &nonce)
        .map_err(|error| SesameError::Corruption(format!("encrypted data page: {error}")))
}
