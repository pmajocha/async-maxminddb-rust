use std::net::IpAddr;
use std::str::FromStr;

use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncSeek};

use super::{MaxMindDBError, Reader};

#[allow(clippy::float_cmp)]
#[tokio::test]
async fn test_decoder() {
    let _ = env_logger::try_init();

    #[allow(non_snake_case)]
    #[derive(Deserialize, Debug, Eq, PartialEq)]
    struct MapXType {
        arrayX: Vec<u32>,
        utf8_stringX: String,
    }

    #[allow(non_snake_case)]
    #[derive(Deserialize, Debug, Eq, PartialEq)]
    struct MapType {
        mapX: MapXType,
    }

    #[derive(Deserialize, Debug)]
    struct TestType {
        array: Vec<u32>,
        boolean: bool,
        //bytes: Vec<u8>,
        double: f64,
        float: f32,
        int32: i32,
        map: MapType,
        uint16: u16,
        uint32: u32,
        uint64: u64,
        uint128: u128,
        utf8_string: String,
    }

    let r = Reader::open_readfile("test-data/test-data/MaxMind-DB-test-decoder.mmdb").await;
    if let Err(err) = r {
        panic!("error opening mmdb: {:?}", err);
    }
    let mut r = r.unwrap();
    let ip: IpAddr = FromStr::from_str("1.1.1.0").unwrap();
    let result: TestType = r.lookup(ip).await.unwrap();

    assert_eq!(result.array, vec![1_u32, 2_u32, 3_u32]);
    assert!(result.boolean);
    //assert_eq!(result.bytes, vec![0_u8, 0_u8, 0_u8, 42_u8]);
    assert_eq!(result.double, 42.123_456);
    assert_eq!(result.float, 1.1);
    assert_eq!(result.int32, -268_435_456);

    assert_eq!(
        result.map,
        MapType {
            mapX: MapXType {
                arrayX: vec![7, 8, 9],
                utf8_stringX: "hello".to_string(),
            },
        }
    );

    assert_eq!(result.uint16, 100);
    assert_eq!(result.uint32, 268_435_456);
    assert_eq!(result.uint64, 1_152_921_504_606_846_976);
    assert_eq!(
        result.uint128,
        1_329_227_995_784_915_872_903_807_060_280_344_576
    );

    assert_eq!(
        result.utf8_string,
        "unicode! \u{262f} - \u{266b}".to_string()
    );
}

#[tokio::test]
async fn test_pointers_in_metadata() {
    let _ = env_logger::try_init();

    let r = Reader::open_readfile("test-data/test-data/MaxMind-DB-test-metadata-pointers.mmdb");
    if let Err(err) = r.await {
        panic!("error opening mmdb: {:?}", err);
    }
}

#[tokio::test]
async fn test_broken_database() {
    let _ = env_logger::try_init();

    let mut r = Reader::open_readfile("test-data/test-data/GeoIP2-City-Test-Broken-Double-Format.mmdb")
        .await
        .ok()
        .unwrap();
    let ip: IpAddr = FromStr::from_str("2001:220::").unwrap();

    #[derive(Deserialize, Debug)]
    struct TestType {}
    match r.lookup::<TestType>(ip).await {
        Err(e) => assert_eq!(
            e,
            MaxMindDBError::InvalidDatabaseError("double of size 2".to_string())
        ),
        Ok(_) => panic!("Error expected"),
    }
}

#[tokio::test]
async fn test_missing_database() {
    let _ = env_logger::try_init();

    let r = Reader::open_readfile("file-does-not-exist.mmdb").await;
    match r {
        Ok(_) => panic!("Received Reader when opening non-existent file"),
        Err(e) => assert!(
            e == MaxMindDBError::IoError(
                "The system cannot find the file specified. (os error 2)".to_string()
            ) || e == MaxMindDBError::IoError("No such file or directory (os error 2)".to_string())
        ),
    }
}

#[tokio::test]
async fn test_non_database() {
    let _ = env_logger::try_init();

    let r = Reader::open_readfile("README.md").await;
    match r {
        Ok(_) => panic!("Received Reader when opening a non-MMDB file"),
        Err(e) => assert_eq!(
            e,
            MaxMindDBError::InvalidDatabaseError(
                "Could not find MaxMind DB metadata \
                 in file."
                    .to_string(),
            )
        ),
    }
}

#[tokio::test]
async fn test_reader() {
    let _ = env_logger::try_init();

    let sizes = [24_usize, 28, 32];
    for record_size in &sizes {
        let versions = [4_usize, 6];
        for ip_version in &versions {
            let filename = format!(
                "test-data/test-data/MaxMind-DB-test-ipv{}-{}.mmdb",
                ip_version, record_size
            );
            let mut reader = Reader::open_readfile(&filename).await.ok().unwrap();

            check_metadata(&reader, *ip_version, *record_size);
            check_ip(&mut reader, *ip_version).await;
        }
    }
}

/// Create Reader by explicitly reading the entire file into a buffer.
#[tokio::test]
async fn test_reader_readfile() {
    let _ = env_logger::try_init();

    let sizes = [24_usize, 28, 32];
    for record_size in &sizes {
        let versions = [4_usize, 6];
        for ip_version in &versions {
            let filename = format!(
                "test-data/test-data/MaxMind-DB-test-ipv{}-{}.mmdb",
                ip_version, record_size
            );
            let mut reader = Reader::open_readfile(&filename).await.ok().unwrap();

            check_metadata(&reader, *ip_version, *record_size);
            check_ip(&mut reader, *ip_version).await;
        }
    }
}

#[tokio::test]
#[cfg(feature = "mmap")]
fn test_reader_mmap() {
    let _ = env_logger::try_init();

    let sizes = [24usize, 28, 32];
    for record_size in sizes.iter() {
        let versions = [4usize, 6];
        for ip_version in versions.iter() {
            let filename = format!(
                "test-data/test-data/MaxMind-DB-test-ipv{}-{}.mmdb",
                ip_version, record_size
            );
            let mut reader = Reader::open_mmap(filename).ok().unwrap();

            check_metadata(&reader, *ip_version, *record_size);
            check_ip(&reader, *ip_version);
        }
    }
}

#[tokio::test]
async fn test_lookup_city() {
    use super::geoip2::City;
    let _ = env_logger::try_init();

    let filename = "test-data/test-data/GeoIP2-City-Test.mmdb";

    let mut reader = Reader::open_readfile(filename).await.unwrap();

    let ip: IpAddr = FromStr::from_str("89.160.20.112").unwrap();
    let city: City = reader.lookup(ip).await.unwrap();

    let iso_code = city.country.and_then(|cy| cy.iso_code);

    assert_eq!(iso_code, Some("SE".to_owned()));
}

#[tokio::test]
async fn test_lookup_country() {
    use super::geoip2::Country;
    let _ = env_logger::try_init();

    let filename = "test-data/test-data/GeoIP2-Country-Test.mmdb";

    let mut reader = Reader::open_readfile(filename).await.unwrap();

    let ip: IpAddr = FromStr::from_str("89.160.20.112").unwrap();
    let country: Country = reader.lookup(ip).await.unwrap();
    let country = country.country.unwrap();

    assert_eq!(country.iso_code, Some("SE".to_owned()));
    assert_eq!(country.is_in_european_union, Some(true));
}

#[tokio::test]
async fn test_lookup_connection_type() {
    use super::geoip2::ConnectionType;
    let _ = env_logger::try_init();

    let filename = "test-data/test-data/GeoIP2-Connection-Type-Test.mmdb";

    let mut reader = Reader::open_readfile(filename).await.unwrap();

    let ip: IpAddr = FromStr::from_str("96.1.20.112").unwrap();
    let connection_type: ConnectionType = reader.lookup(ip).await.unwrap();

    assert_eq!(connection_type.connection_type, Some("Cable/DSL".to_owned()));
}

#[tokio::test]
async fn test_lookup_annonymous_ip() {
    use super::geoip2::AnonymousIp;
    let _ = env_logger::try_init();

    let filename = "test-data/test-data/GeoIP2-Anonymous-IP-Test.mmdb";

    let mut reader = Reader::open_readfile(filename).await.unwrap();

    let ip: IpAddr = FromStr::from_str("81.2.69.123").unwrap();
    let anonymous_ip: AnonymousIp = reader.lookup(ip).await.unwrap();

    assert_eq!(anonymous_ip.is_anonymous, Some(true));
    assert_eq!(anonymous_ip.is_public_proxy, Some(true));
    assert_eq!(anonymous_ip.is_anonymous_vpn, Some(true));
    assert_eq!(anonymous_ip.is_hosting_provider, Some(true));
    assert_eq!(anonymous_ip.is_tor_exit_node, Some(true))
}

#[tokio::test]
async fn test_lookup_density_income() {
    use super::geoip2::DensityIncome;
    let _ = env_logger::try_init();

    let filename = "test-data/test-data/GeoIP2-DensityIncome-Test.mmdb";

    let mut reader = Reader::open_readfile(filename).await.unwrap();

    let ip: IpAddr = FromStr::from_str("5.83.124.123").unwrap();
    let density_income: DensityIncome = reader.lookup(ip).await.unwrap();

    assert_eq!(density_income.average_income, Some(32323));
    assert_eq!(density_income.population_density, Some(1232))
}

#[tokio::test]
async fn test_lookup_domain() {
    use super::geoip2::Domain;
    let _ = env_logger::try_init();

    let filename = "test-data/test-data/GeoIP2-Domain-Test.mmdb";

    let mut reader = Reader::open_readfile(filename).await.unwrap();

    let ip: IpAddr = FromStr::from_str("66.92.80.123").unwrap();
    let domain: Domain = reader.lookup(ip).await.unwrap();

    assert_eq!(domain.domain, Some("speakeasy.net".to_owned()));
}

#[tokio::test]
async fn test_lookup_isp() {
    use super::geoip2::Isp;
    let _ = env_logger::try_init();

    let filename = "test-data/test-data/GeoIP2-ISP-Test.mmdb";

    let mut reader = Reader::open_readfile(filename).await.unwrap();

    let ip: IpAddr = FromStr::from_str("12.87.118.123").unwrap();
    let isp: Isp = reader.lookup(ip).await.unwrap();

    assert_eq!(isp.autonomous_system_number, Some(7018));
    assert_eq!(isp.isp, Some("AT&T Services".to_owned()));
    assert_eq!(isp.organization, Some("AT&T Worldnet Services".to_owned()));
}

#[tokio::test]
async fn test_lookup_asn() {
    use super::geoip2::Asn;
    let _ = env_logger::try_init();

    let filename = "test-data/test-data/GeoIP2-ISP-Test.mmdb";

    let mut reader = Reader::open_readfile(filename).await.unwrap();

    let ip: IpAddr = FromStr::from_str("1.128.0.123").unwrap();
    let asn: Asn = reader.lookup(ip).await.unwrap();

    assert_eq!(asn.autonomous_system_number, Some(1221));
    assert_eq!(asn.autonomous_system_organization, Some("Telstra Pty Ltd".to_owned()));
}

#[tokio::test]
async fn test_lookup_prefix() {
    use super::geoip2::City;
    let _ = env_logger::try_init();

    let filename = "test-data/test-data/GeoIP2-ISP-Test.mmdb";

    let mut reader = Reader::open_readfile(filename).await.unwrap();

    // IPv4
    let ip: IpAddr = "89.160.20.128".parse().unwrap();
    let (_, prefix_len) = reader.lookup_prefix::<City>(ip).await.unwrap();

    assert_eq!(prefix_len, 25); // "::89.160.20.128/121"

    // Last host
    let ip: IpAddr = "89.160.20.254".parse().unwrap();
    let (_, last_prefix_len) = reader.lookup_prefix::<City>(ip).await.unwrap();

    assert_eq!(prefix_len, last_prefix_len);

    // IPv6
    let ip: IpAddr = "2c0f:ff00::1".parse().unwrap();
    let (_, prefix_len) = reader.lookup_prefix::<City>(ip).await.unwrap();

    assert_eq!(prefix_len, 26); // "2c0f:ff00::/26"
}

fn check_metadata<T: AsyncRead + AsyncSeek + Unpin>(reader: &Reader<T>, ip_version: usize, record_size: usize) {
    let metadata = &reader.metadata;

    assert_eq!(metadata.binary_format_major_version, 2_u16);

    assert_eq!(metadata.binary_format_minor_version, 0_u16);
    assert!(metadata.build_epoch >= 1_397_457_605);
    assert_eq!(metadata.database_type, "Test".to_string());

    assert_eq!(
        *metadata.description[&"en".to_string()],
        "Test Database".to_string()
    );
    assert_eq!(
        *metadata.description[&"zh".to_string()],
        "Test Database Chinese".to_string()
    );

    assert_eq!(metadata.ip_version, ip_version as u16);
    assert_eq!(metadata.languages, vec!["en".to_string(), "zh".to_string()]);

    if ip_version == 4 {
        assert_eq!(metadata.node_count, 164)
    } else {
        assert_eq!(metadata.node_count, 416)
    }

    assert_eq!(metadata.record_size, record_size as u16)
}

async fn check_ip<T: AsyncRead + AsyncSeek + Unpin>(reader: &mut Reader<T>, ip_version: usize) {
    let subnets = match ip_version {
        6 => [
            "::1:ffff:ffff",
            "::2:0:0",
            "::2:0:0",
            "::2:0:0",
            "::2:0:0",
            "::2:0:40",
            "::2:0:40",
            "::2:0:40",
            "::2:0:50",
            "::2:0:50",
            "::2:0:50",
            "::2:0:58",
            "::2:0:58",
        ],
        _ => [
            "1.1.1.1", "1.1.1.2", "1.1.1.2", "1.1.1.4", "1.1.1.4", "1.1.1.4", "1.1.1.4", "1.1.1.8",
            "1.1.1.8", "1.1.1.8", "1.1.1.16", "1.1.1.16", "1.1.1.16",
        ],
    };

    #[derive(Deserialize, Debug)]
    struct IpType {
        ip: String,
    }

    for subnet in &subnets {
        let ip: IpAddr = FromStr::from_str(subnet).unwrap();
        let value: IpType = reader.lookup(ip).await.unwrap();

        assert_eq!(value.ip, *subnet);
    }

    let no_record = ["1.1.1.33", "255.254.253.123", "89fa::"];

    for &address in &no_record {
        let ip: IpAddr = FromStr::from_str(address).unwrap();
        match reader.lookup::<IpType>(ip).await {
            Ok(v) => panic!("received an unexpected value: {:?}", v),
            Err(e) => assert_eq!(
                e,
                MaxMindDBError::AddressNotFoundError("Address not found in database".to_string())
            ),
        }
    }
}
