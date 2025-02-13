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
pub struct Reader<S: AsyncRead + AsyncSeek + Unpin> {
    source: Source<S>,
    pub metadata: Metadata,
    ipv4_start: usize,
    pointer_base: usize,
}

impl Reader<File> {
    pub async fn open_readfile(database: &str) -> Result<Reader<File>, MaxMindDBError> {
        let source = Source::new(database).await?;
        Ok(Reader::from_source(source).await?)
    }
}

impl<'de, S: AsyncRead + AsyncSeek + Unpin> Reader<S> {
    pub async fn from_source(mut source: Source<S>) -> Result<Reader<S>, MaxMindDBError> {
        let data_section_separator_size = 16;

        let metadata_start = find_metadata_start(&mut source).await?;
        source.move_cursor(metadata_start as u64).await?;

        let metadata = try_decode_increasing_buffer(&mut source, 0, |buf| {
            let mut type_decoder = decoder::Decoder::new(buf, 0);
            Metadata::deserialize(&mut type_decoder).ok()    
        })
        .await?
        .ok_or_else(|| MaxMindDBError::DecodingError("Couldn't decode Metadata".to_owned()))?;

        let search_tree_size = (metadata.node_count as usize) * (metadata.record_size as usize) / 4;

        let mut reader = Reader {
            source,
            pointer_base: search_tree_size + data_section_separator_size,
            metadata,
            ipv4_start: 0,
        };
        reader.ipv4_start = reader.find_ipv4_start().await?;

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
    pub async fn lookup<T>(&mut self, address: IpAddr) -> Result<T, MaxMindDBError>
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
    pub async fn lookup_prefix<T>(&mut self, address: IpAddr) -> Result<(T, usize), MaxMindDBError>
    where
        T: DeserializeOwned,
    {
        let ip_bytes = ip_to_bytes(address);
        let (pointer, prefix_len) = self.find_address_in_tree(&ip_bytes).await?;
        if pointer == 0 {
            return Err(MaxMindDBError::AddressNotFoundError(
                "Address not found in database".to_owned(),
            ));
        }

        let rec = self.resolve_data_pointer(pointer)?;
        self.source.move_cursor(self.pointer_base as u64).await?;

        try_decode_increasing_buffer(&mut self.source, rec, |buf| {
            let mut decoder = decoder::Decoder::new(buf, rec);
            T::deserialize(&mut decoder).map(|v| (v, prefix_len)).map_err(|e| dbg!(e)).ok()
        })
        .await?
        .ok_or_else(|| MaxMindDBError::DecodingError(format!("Error decoding {}", std::any::type_name::<T>())))
    }

    async fn find_address_in_tree(&mut self, ip_address: &[u8]) -> Result<(usize, usize), MaxMindDBError> {
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

            node = self.read_node(node, bit as usize).await?;
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

    async fn find_ipv4_start(&mut self) -> Result<usize, MaxMindDBError> {
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
            node = self.read_node(node, 0).await?;
        }
        Ok(node)
    }

    async fn read_node(&mut self, node_number: usize, index: usize) -> Result<usize, MaxMindDBError> {
        let base_offset = node_number * (self.metadata.record_size as usize) / 4;

        let val = match self.metadata.record_size {
            24 => {
                let offset = base_offset + index * 3;
                to_usize(0, self.source.read_at(offset as u64, 3).await?) // &buf[offset..offset + 3]
            }
            28 => {
                let mut middle = self.source.read_one(base_offset as u64 + 3).await?;
                if index != 0 {
                    middle &= 0x0F
                } else {
                    middle = (0xF0 & middle) >> 4
                }
                let offset = base_offset + index * 4;
                to_usize(middle, self.source.read_at(offset as u64, 3).await?) //&buf[offset..offset + 3])
            }
            32 => {
                let offset = base_offset + index * 4;
                to_usize(0, self.source.read_at(offset as u64, 4).await?) // &buf[offset..offset + 4])
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

    fn resolve_data_pointer(&self, pointer: usize) -> Result<usize, MaxMindDBError> {
        let resolved = pointer - (self.metadata.node_count as usize) - 16;
        
        if resolved > self.source.total_size {
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

async fn find_metadata_start<S: AsyncRead + AsyncSeek + Unpin>(source: &mut Source<S>) -> Result<usize, MaxMindDBError> {
    const METADATA_START_MARKER: &[u8] = b"\xab\xcd\xefMaxMind.com";

    try_decode_increasing_buffer(source, 0, |buf| 
        memchr::memmem::rfind(buf, METADATA_START_MARKER)
        .map(|idx| idx + METADATA_START_MARKER.len()))
    .await?
    .ok_or_else(|| MaxMindDBError::InvalidDatabaseError(
        "Could not find MaxMind DB metadata in file.".to_owned(),
    ))
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
    use super::MaxMindDBError;

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
