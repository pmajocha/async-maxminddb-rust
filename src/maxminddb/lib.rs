#![deny(trivial_casts, trivial_numeric_casts, unused_import_braces)]

use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::net::IpAddr;

use serde::de::DeserializeOwned;
use serde::{de, Deserialize};
use source::Source;
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncSeek};

#[derive(Debug, PartialEq, Eq)]
pub enum MaxMindDBError {
    AddressNotFoundError(String),
    InvalidDatabaseError(String),
    IoError(String),
    MapError(String),
    DecodingError(String),
    InvalidNetworkError(String),
}

impl From<io::Error> for MaxMindDBError {
    fn from(err: io::Error) -> MaxMindDBError {
        // clean up and clean up MaxMindDBError generally
        MaxMindDBError::IoError(err.to_string())
    }
}

impl Display for MaxMindDBError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            MaxMindDBError::AddressNotFoundError(msg) => {
                write!(fmt, "AddressNotFoundError: {}", msg)?
            }
            MaxMindDBError::InvalidDatabaseError(msg) => {
                write!(fmt, "InvalidDatabaseError: {}", msg)?
            }
            MaxMindDBError::IoError(msg) => write!(fmt, "IoError: {}", msg)?,
            MaxMindDBError::MapError(msg) => write!(fmt, "MapError: {}", msg)?,
            MaxMindDBError::DecodingError(msg) => write!(fmt, "DecodingError: {}", msg)?,
            MaxMindDBError::InvalidNetworkError(msg) => {
                write!(fmt, "InvalidNetworkError: {}", msg)?
            }
        }
        Ok(())
    }
}

// Use default implementation for `std::error::Error`
impl std::error::Error for MaxMindDBError {}

impl de::Error for MaxMindDBError {
    fn custom<T: Display>(msg: T) -> Self {
        MaxMindDBError::DecodingError(format!("{}", msg))
    }
}

#[derive(Deserialize, Debug)]
pub struct Metadata {
    pub binary_format_major_version: u16,
    pub binary_format_minor_version: u16,
    pub build_epoch: u64,
    pub database_type: String,
    pub description: BTreeMap<String, String>,
    pub ip_version: u16,
    pub languages: Vec<String>,
    pub node_count: u32,
    pub record_size: u16,
}

/// A reader for the MaxMind DB format. The lifetime `'data` is tied to the lifetime of the underlying buffer holding the contents of the database file.
pub struct Reader {
    source_path: String,
    pub metadata: Metadata,
    ipv4_start: usize,
    pointer_base: usize,
}

impl Reader {
    pub async fn open_readfile(database: &str) -> Result<Reader, MaxMindDBError> {
        Ok(Reader::from_source(database.to_owned()).await?)
    }
}

#[allow(dead_code)]
async fn find_meta_by_loading_whole_file(s: &str) -> Result<usize, MaxMindDBError> {
    let buf = tokio::fs::read(s).await?;
    memchr::memmem::rfind(&buf, METADATA_START_MARKER)
        .map(|m| m + METADATA_START_MARKER.len())
        .ok_or_else(|| MaxMindDBError::InvalidDatabaseError(
            "Could not find MaxMind DB metadata in file.".to_owned()))
}

impl Reader {
    pub async fn from_source(source_path: String) -> Result<Reader, MaxMindDBError> {
        let data_section_separator_size = 16;
        let mut source = Source::new(&source_path).await?;

        const CHUNK: usize = 1024 * 1024; // 1 Mb
        let metadata_start = find_metadata_start(&mut source, CHUNK).await?;
        source.move_cursor(metadata_start as u64).await?;

        let metadata = try_decode_increasing_buffer(&mut source, 0, |buf| {
            let mut type_decoder = decoder::Decoder::new(buf, 0);
            Metadata::deserialize(&mut type_decoder).ok()    
        })
        .await?
        .ok_or_else(|| MaxMindDBError::DecodingError("Couldn't decode Metadata".to_owned()))?;

        let search_tree_size = (metadata.node_count as usize) * (metadata.record_size as usize) / 4;

        let mut reader = Reader {
            source_path,
            pointer_base: search_tree_size + data_section_separator_size,
            metadata,
            ipv4_start: 0,
        };

        reader.ipv4_start = reader.find_ipv4_start(&mut source).await?;

        Ok(reader)
    }

    /// Lookup the socket address in the opened MaxMind DB
    ///
    /// Example:
    ///
    /// ```
    /// use maxminddb::geoip2;
    /// use std::net::IpAddr;
    /// use std::str::FromStr;
    ///
    /// let reader = maxminddb::Reader::open_readfile("test-data/test-data/GeoIP2-City-Test.mmdb").unwrap();
    ///
    /// let ip: IpAddr = FromStr::from_str("89.160.20.128").unwrap();
    /// let city: geoip2::City = reader.lookup(ip).unwrap();
    /// print!("{:?}", city);
    /// ```
    pub async fn lookup<T>(&self, address: IpAddr) -> Result<T, MaxMindDBError>
    where
        T: DeserializeOwned,
    {
        self.lookup_prefix(address).await.map(|(v, _)| v)
    }

    /// Lookup the socket address in the opened MaxMind DB
    ///
    /// Example:
    ///
    /// ```
    /// use maxminddb::geoip2;
    /// use std::net::IpAddr;
    /// use std::str::FromStr;
    ///
    /// let reader = maxminddb::Reader::open_readfile("test-data/test-data/GeoIP2-City-Test.mmdb").unwrap();
    ///
    /// let ip: IpAddr = "89.160.20.128".parse().unwrap();
    /// let (city, prefix_len) = reader.lookup_prefix::<geoip2::City>(ip).unwrap();
    /// print!("{:?}, prefix length: {}", city, prefix_len);
    /// ```
    pub async fn lookup_prefix<T>(&self, address: IpAddr) -> Result<(T, usize), MaxMindDBError>
    where
        T: DeserializeOwned,
    {
        let mut source = Source::new(&self.source_path).await?;
        let ip_bytes = ip_to_bytes(address);
        let (pointer, prefix_len) = self.find_address_in_tree(&mut source, &ip_bytes).await?;
        if pointer == 0 {
            return Err(MaxMindDBError::AddressNotFoundError(
                "Address not found in database".to_owned(),
            ));
        }

        let rec = self.resolve_data_pointer(pointer, source.total_size)?;
        source.move_cursor(self.pointer_base as u64).await?;

        //TODO rec can be quite big, need to investigate how data is read, because it happens, that we load 3 Mb into memory, but then use only 1 kB of it
        try_decode_increasing_buffer(&mut source, rec, |buf| {
            let mut decoder = decoder::Decoder::new(buf, rec);
            T::deserialize(&mut decoder).map(|v| (v, prefix_len)).map_err(|e| dbg!(e)).ok()
        })
        .await?
        .ok_or_else(|| MaxMindDBError::DecodingError(format!("Error decoding {}", std::any::type_name::<T>())))
    }

    async fn find_address_in_tree<S: AsyncRead + AsyncSeek + Unpin>(&self, source: &mut Source<S>, ip_address: &[u8]) -> Result<(usize, usize), MaxMindDBError> {
        let bit_count = ip_address.len() * 8;
        let mut node = self.start_node(bit_count);

        let node_count = self.metadata.node_count as usize;
        let mut prefix_len = bit_count;

        for i in 0..bit_count {
            if node >= node_count {
                prefix_len = i;
                break;
            }
            let bit = 1 & (ip_address[i >> 3] >> (7 - (i % 8)));

            node = self.read_node(source, node, bit as usize).await?;
        }
        match node_count {
            n if n == node => Ok((0, prefix_len)),
            n if node > n => Ok((node, prefix_len)),
            _ => Err(MaxMindDBError::InvalidDatabaseError(
                "invalid node in search tree".to_owned(),
            )),
        }
    }

    fn start_node(&self, length: usize) -> usize {
        if length == 128 {
            0
        } else {
            self.ipv4_start
        }
    }

    async fn find_ipv4_start<S: AsyncRead + AsyncSeek + Unpin>(&self, source: &mut Source<S>) -> Result<usize, MaxMindDBError> {
        if self.metadata.ip_version != 6 {
            return Ok(0);
        }

        // We are looking up an IPv4 address in an IPv6 tree. Skip over the
        // first 96 nodes.
        let mut node: usize = 0_usize;
        for _ in 0_u8..96 {
            if node >= self.metadata.node_count as usize {
                break;
            }
            node = self.read_node(source, node, 0).await?;
        }
        Ok(node)
    }

    async fn read_node<S: AsyncRead + AsyncSeek + Unpin>(&self, source: &mut Source<S>, node_number: usize, index: usize) -> Result<usize, MaxMindDBError> {
        let base_offset = node_number * (self.metadata.record_size as usize) / 4;

        let val = match self.metadata.record_size {
            24 => {
                let offset = base_offset + index * 3;
                to_usize(0, source.read_at(offset as u64, 3).await?) // &buf[offset..offset + 3]
            }
            28 => {
                let mut middle = source.read_one(base_offset as u64 + 3).await?;
                if index != 0 {
                    middle &= 0x0F
                } else {
                    middle = (0xF0 & middle) >> 4
                }
                let offset = base_offset + index * 4;
                to_usize(middle, source.read_at(offset as u64, 3).await?) //&buf[offset..offset + 3])
            }
            32 => {
                let offset = base_offset + index * 4;
                to_usize(0, source.read_at(offset as u64, 4).await?) // &buf[offset..offset + 4])
            }
            s => {
                return Err(MaxMindDBError::InvalidDatabaseError(format!(
                    "unknown record size: \
                     {:?}",
                    s
                )))
            }
        };
        Ok(val)
    }

    fn resolve_data_pointer(&self, pointer: usize, total_size: usize) -> Result<usize, MaxMindDBError> {
        let resolved = pointer - (self.metadata.node_count as usize) - 16;
        
        if resolved > total_size {
            return Err(MaxMindDBError::InvalidDatabaseError(
                "the MaxMind DB file's search tree \
                 is corrupt"
                    .to_owned(),
            ));
        }

        Ok(resolved)
    }
}

// I haven't moved all patterns of this form to a generic function as
// the FromPrimitive trait is unstable
fn to_usize(base: u8, bytes: &[u8]) -> usize {
    bytes
        .iter()
        .fold(base as usize, |acc, &b| (acc << 8) | b as usize)
}

fn ip_to_bytes(address: IpAddr) -> Vec<u8> {
    match address {
        IpAddr::V4(a) => a.octets().to_vec(),
        IpAddr::V6(a) => a.octets().to_vec(),
    }
}

const METADATA_START_MARKER: &[u8] = b"\xab\xcd\xefMaxMind.com";
const MARKER_LEN: usize = METADATA_START_MARKER.len();
/// Find metadata start by taking 1Mb chunks of data.
/// Chunks are then connected to check if metadata start marker is between them.
async fn find_metadata_start<S: AsyncRead + AsyncSeek + Unpin>(source: &mut Source<S>, chunk_size: usize) -> Result<usize, MaxMindDBError> {
    let mut iter = source.total_size;
    let mut last_buf = vec![];
    let chunk_size = usize::max(chunk_size, MARKER_LEN);

    loop {
        if let Some(start) = iter.checked_sub(chunk_size) {
            source.move_cursor(start as u64).await?;
            let current_buf = source.read(chunk_size).await?;
            if let Some(meta_start) = try_find_metadata(start, chunk_size, current_buf, &last_buf) {
                return Ok(meta_start);
            }
            last_buf = current_buf[..MARKER_LEN].to_vec();
            iter -= chunk_size;
        } else {
            // only [0..iter-1] left or [0..iter] if file is smaller than 1 Mb
            source.move_cursor(0).await?;
            let to_read = if iter == source.total_size { iter } else { iter - 1 };
            let current_buf = source.read(to_read).await?;
            return try_find_metadata(0,  to_read, current_buf, &last_buf)
                .ok_or_else(|| MaxMindDBError::InvalidDatabaseError(
                    "Could not find MaxMind DB metadata in file.".to_owned()))
        }
    }
}

fn try_find_metadata(start: usize, chunk_size: usize, current_buf: &[u8], last_buf: &[u8]) -> Option<usize> {
    if let Some(meta_start) = check_in_buf(current_buf) {
        return Some(start + meta_start);
    }
    if let Some(meta_start) = check_between(&last_buf, &current_buf) {
        return Some(start + (chunk_size - MARKER_LEN) + meta_start);
    }
    None
}

fn check_between(last_buf: &[u8], current_buf: &[u8]) -> Option<usize> {
    if last_buf.is_empty() {
        return None
    }
    let from_current = &current_buf[(current_buf.len() - MARKER_LEN)..];
    let from_last = &last_buf[..MARKER_LEN];
    let mut result = [0; MARKER_LEN * 2];
    result[..MARKER_LEN].copy_from_slice(from_current);
    result[MARKER_LEN..].copy_from_slice(from_last);
    check_in_buf(&result)
}

fn check_in_buf(buf: &[u8]) -> Option<usize> {
    memchr::memmem::rfind(&buf, METADATA_START_MARKER)
        .map(|idx| idx + METADATA_START_MARKER.len())
}

async fn try_decode_increasing_buffer<S, F, O>(source: &mut Source<S>, rec: usize, f: F) -> Result<Option<O>, MaxMindDBError> 
where
    S: AsyncRead + AsyncSeek + Unpin,
    F: Fn(&[u8]) -> Option<O>,
{
    const BASE: usize = 1024;
    let start_position = source.position().await?;
    let max_size = source.total_size - start_position as usize;

    for size_mult in 1..usize::MAX {
        source.move_cursor(start_position).await?;
        if rec + size_mult * BASE > max_size {
            let buf = source.read(max_size).await?;
            return Ok(f(buf))
        }
        let buf = source.read(rec + size_mult * BASE).await?;
        if let Some(out) = f(buf) {
            return Ok(Some(out))
        }
    }
    Ok(None)
}

mod decoder;
mod source;
pub mod geoip2;

#[cfg(test)]
mod reader_test;

#[cfg(test)]
mod tests {
    use crate::{source::Source, MARKER_LEN, find_metadata_start};

    use super::MaxMindDBError;

    #[tokio::test]
    async fn test_reading_metadata() {
        let path = "test-data/test-data/MaxMind-DB-test-decoder.mmdb";
        let mut source = Source::new(path).await.unwrap();

        for size in MARKER_LEN..(1024 * 32) {
            assert_eq!(2931, find_metadata_start(&mut source, size).await.unwrap());
        }
    }

    #[test]
    fn test_error_display() {
        assert_eq!(
            format!(
                "{}",
                MaxMindDBError::AddressNotFoundError("something went wrong".to_owned())
            ),
            "AddressNotFoundError: something went wrong".to_owned(),
        );
        assert_eq!(
            format!(
                "{}",
                MaxMindDBError::InvalidDatabaseError("something went wrong".to_owned())
            ),
            "InvalidDatabaseError: something went wrong".to_owned(),
        );
        assert_eq!(
            format!(
                "{}",
                MaxMindDBError::IoError("something went wrong".to_owned())
            ),
            "IoError: something went wrong".to_owned(),
        );
        assert_eq!(
            format!(
                "{}",
                MaxMindDBError::MapError("something went wrong".to_owned())
            ),
            "MapError: something went wrong".to_owned(),
        );
        assert_eq!(
            format!(
                "{}",
                MaxMindDBError::DecodingError("something went wrong".to_owned())
            ),
            "DecodingError: something went wrong".to_owned(),
        );
    }
}
