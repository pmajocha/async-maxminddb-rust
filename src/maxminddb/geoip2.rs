use serde::{Deserialize, Serialize};

/// GeoIP2 Country record
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Country {
    pub continent: Option<country::Continent>,
    pub country: Option<country::Country>,
    pub registered_country: Option<country::Country>,
    pub represented_country: Option<country::RepresentedCountry>,
    pub traits: Option<country::Traits>,
}

/// GeoIP2 City record
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct City {
    pub city: Option<city::City>,
    pub continent: Option<city::Continent>,
    pub country: Option<city::Country>,
    pub location: Option<city::Location>,
    pub postal: Option<city::Postal>,
    pub registered_country: Option<city::Country>,
    pub represented_country: Option<city::RepresentedCountry>,
    pub subdivisions: Option<Vec<city::Subdivision>>,
    pub traits: Option<city::Traits>,
}

/// GeoIP2 Enterprise record
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Enterprise {
    pub city: Option<enterprise::City>,
    pub continent: Option<enterprise::Continent>,
    pub country: Option<enterprise::Country>,
    pub location: Option<enterprise::Location>,
    pub postal: Option<enterprise::Postal>,
    pub registered_country: Option<enterprise::Country>,
    pub represented_country: Option<enterprise::RepresentedCountry>,
    pub subdivisions: Option<Vec<enterprise::Subdivision>>,
    pub traits: Option<enterprise::Traits>,
}

/// GeoIP2 ISP record
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Isp {
    pub autonomous_system_number: Option<u32>,
    pub autonomous_system_organization: Option<String>,
    pub isp: Option<String>,
    pub mobile_country_code: Option<String>,
    pub mobile_network_code: Option<String>,
    pub organization: Option<String>,
}

/// GeoIP2 Connection-Type record
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ConnectionType {
    pub connection_type: Option<String>,
}

/// GeoIP2 Anonymous Ip record
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct AnonymousIp {
    pub is_anonymous: Option<bool>,
    pub is_anonymous_vpn: Option<bool>,
    pub is_hosting_provider: Option<bool>,
    pub is_public_proxy: Option<bool>,
    pub is_residential_proxy: Option<bool>,
    pub is_tor_exit_node: Option<bool>,
}

/// GeoIP2 DensityIncome record
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DensityIncome {
    pub average_income: Option<u32>,
    pub population_density: Option<u32>,
}

/// GeoIP2 Domain record
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Domain {
    pub domain: Option<String>,
}

/// GeoIP2 Asn record
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Asn {
    pub autonomous_system_number: Option<u32>,
    pub autonomous_system_organization: Option<String>,
}

/// Country model structs
pub mod country {
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct Continent {
        pub code: Option<String>,
        pub geoname_id: Option<u32>,
        pub names: Option<BTreeMap<String, String>>,
    }

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct Country {
        pub geoname_id: Option<u32>,
        pub is_in_european_union: Option<bool>,
        pub iso_code: Option<String>,
        pub names: Option<BTreeMap<String, String>>,
    }

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct RepresentedCountry {
        pub geoname_id: Option<u32>,
        pub is_in_european_union: Option<bool>,
        pub iso_code: Option<String>,
        pub names: Option<BTreeMap<String, String>>,
        #[serde(rename = "type")]
        pub representation_type: Option<String>,
    }

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct Traits {
        pub is_anonymous_proxy: Option<bool>,
        pub is_satellite_provider: Option<bool>,
    }
}

/// Country model structs
pub mod city {
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    pub use super::country::{Continent, Country, RepresentedCountry, Traits};

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct City {
        pub geoname_id: Option<u32>,
        pub names: Option<BTreeMap<String, String>>,
    }

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct Location {
        pub accuracy_radius: Option<u16>,
        pub latitude: Option<f64>,
        pub longitude: Option<f64>,
        pub metro_code: Option<u16>,
        pub time_zone: Option<String>,
    }

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct Postal {
        pub code: Option<String>,
    }

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct Subdivision {
        pub geoname_id: Option<u32>,
        pub iso_code: Option<String>,
        pub names: Option<BTreeMap<String, String>>,
    }
}

/// Enterprise model structs
pub mod enterprise {
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    pub use super::country::{Continent, RepresentedCountry};

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct City {
        pub confidence: Option<u8>,
        pub geoname_id: Option<u32>,
        pub names: Option<BTreeMap<String, String>>,
    }

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct Country {
        pub confidence: Option<u8>,
        pub geoname_id: Option<u32>,
        pub is_in_european_union: Option<bool>,
        pub iso_code: Option<String>,
        pub names: Option<BTreeMap<String, String>>,
    }

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct Location {
        pub accuracy_radius: Option<u16>,
        pub latitude: Option<f64>,
        pub longitude: Option<f64>,
        pub metro_code: Option<u16>,
        pub time_zone: Option<String>,
    }

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct Postal {
        pub code: Option<String>,
        pub confidence: Option<u8>,
    }

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct Subdivision {
        pub confidence: Option<u8>,
        pub geoname_id: Option<u32>,
        pub iso_code: Option<String>,
        pub names: Option<BTreeMap<String, String>>,
    }

    #[derive(Deserialize, Serialize, Clone, Debug)]
    pub struct Traits {
        pub autonomous_system_number: Option<u32>,
        pub autonomous_system_organization: Option<String>,
        pub connection_type: Option<String>,
        pub domain: Option<String>,
        pub is_anonymous: Option<bool>,
        pub is_anonymous_proxy: Option<bool>,
        pub is_anonymous_vpn: Option<bool>,
        pub is_hosting_provider: Option<bool>,
        pub isp: Option<String>,
        pub is_public_proxy: Option<bool>,
        pub is_residential_proxy: Option<bool>,
        pub is_satellite_provider: Option<bool>,
        pub is_tor_exit_node: Option<bool>,
        pub mobile_country_code: Option<String>,
        pub mobile_network_code: Option<String>,
        pub organization: Option<String>,
        pub user_type: Option<String>,
    }
}
