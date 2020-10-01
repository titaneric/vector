use super::Transform;
use crate::{
    config::{DataType, TransformConfig, TransformContext, TransformDescription},
    event::Event,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct GeoipConfig {
    pub source: Atom,
    pub database: String,
    #[serde(default = "default_geoip_target_field")]
    pub target: String,
}

pub struct Geoip {
    pub dbreader: maxminddb::Reader<Vec<u8>>,
    pub source: Atom,
    pub target: String,
}

fn default_geoip_target_field() -> String {
    "geoip".to_string()
}

inventory::submit! {
    TransformDescription::new_without_default::<GeoipConfig>("geoip")
}

#[typetag::serde(name = "geoip")]
impl TransformConfig for GeoipConfig {
    fn build(&self, _cx: TransformContext) -> Result<Box<dyn Transform>, crate::Error> {
        let reader = maxminddb::Reader::open_readfile(self.database.clone())?;
        Ok(Box::new(Geoip::new(
            reader,
            self.source.clone(),
            self.target.clone(),
        )))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "geoip"
    }
}

// MaxMind GeoIP database files have a type field we can use to recognize specific
// products. If we encounter one of these two types, we look for ASN/ISP information;
// otherwise we expect to be working with a City database.
const ASN_DATABASE_TYPE: &str = "GeoLite2-ASN";
const ISP_DATABASE_TYPE: &str = "GeoIP2-ISP";

impl Geoip {
    pub fn new(dbreader: maxminddb::Reader<Vec<u8>>, source: Atom, target: String) -> Self {
        Geoip {
            dbreader,
            source,
            target,
        }
    }

    fn has_isp_db(&self) -> bool {
        self.dbreader.metadata.database_type == ASN_DATABASE_TYPE
            || self.dbreader.metadata.database_type == ISP_DATABASE_TYPE
    }
}

#[derive(Default, Serialize)]
struct ISP<'a> {
    autonomous_system_number: i64,
    autonomous_system_organization: &'a str,
    isp: &'a str,
    organization: &'a str,
}

#[derive(Default, Serialize)]
struct City<'a> {
    city_name: &'a str,
    continent_code: &'a str,
    country_code: &'a str,
    timezone: &'a str,
    latitude: String,  // converted from f64 as per original design
    longitude: String, // converted from f64 as per original design
    postal_code: &'a str,
}

impl Transform for Geoip {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let mut isp: ISP = Default::default();
        let mut city: City = Default::default();
        let target_field = self.target.clone();
        let ipaddress = event
            .as_log()
            .get(&self.source)
            .map(|s| s.to_string_lossy());
        if let Some(ipaddress) = &ipaddress {
            if let Ok(ip) = FromStr::from_str(ipaddress) {
                if self.has_isp_db() {
                    if let Ok(data) = self.dbreader.lookup::<maxminddb::geoip2::Isp>(ip) {
                        if let Some(as_number) = data.autonomous_system_number {
                            isp.autonomous_system_number = as_number as i64;
                        }
                        if let Some(as_organization) = data.autonomous_system_organization {
                            isp.autonomous_system_organization = as_organization;
                        }
                        if let Some(isp_name) = data.isp {
                            isp.isp = isp_name;
                        }
                        if let Some(organization) = data.organization {
                            isp.organization = organization;
                        }
                    }
                } else if let Ok(data) = self.dbreader.lookup::<maxminddb::geoip2::City>(ip) {
                    if let Some(city_names) = data.city.and_then(|c| c.names) {
                        if let Some(city_name) = city_names.get("en") {
                            city.city_name = city_name;
                        }
                    }

                    if let Some(continent_code) = data.continent.and_then(|c| c.code) {
                        city.continent_code = continent_code;
                    }

                    if let Some(country_code) = data.country.and_then(|cy| cy.iso_code) {
                        city.country_code = country_code;
                    };

                    if let Some(time_zone) = data.location.clone().and_then(|loc| loc.time_zone) {
                        city.timezone = time_zone;
                    }

                    if let Some(latitude) = data.location.clone().and_then(|loc| loc.latitude) {
                        city.latitude = latitude.to_string();
                    }

                    if let Some(longitude) = data.location.clone().and_then(|loc| loc.longitude) {
                        city.longitude = longitude.to_string();
                    }

                    if let Some(postal_code) = data.postal.clone().and_then(|p| p.code) {
                        city.postal_code = postal_code;
                    }
                }
            } else {
                debug!(
                    message = "IP Address not parsed correctly.",
                    ipaddr = %ipaddress,
                );
            }
        } else {
            debug!(
                message = "Field does not exist.",
                field = self.source.as_ref(),
            );
        };

        let json_value = if self.has_isp_db() {
            serde_json::to_value(isp)
        } else {
            serde_json::to_value(city)
        };
        if let Ok(json_value) = json_value {
            event.as_mut_log().insert(target_field, json_value);
        }

        Some(event)
    }
}

#[cfg(feature = "transforms-json_parser")]
#[cfg(test)]
mod tests {
    use super::Geoip;
    use crate::{
        event::Event,
        transforms::json_parser::{JsonParser, JsonParserConfig},
        transforms::Transform,
    };
    use std::collections::HashMap;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn geoip_city_lookup_success() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "2.125.160.216", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();
        let reader = maxminddb::Reader::open_readfile("tests/data/GeoIP2-City-Test.mmdb").unwrap();

        let mut augment = Geoip::new(reader, Atom::from("remote_addr"), "geo".to_string());
        let new_event = augment.transform(event).unwrap();

        let mut exp_geoip_attr = HashMap::new();
        exp_geoip_attr.insert("city_name", "Boxford");
        exp_geoip_attr.insert("country_code", "GB");
        exp_geoip_attr.insert("continent_code", "EU");
        exp_geoip_attr.insert("timezone", "Europe/London");
        exp_geoip_attr.insert("latitude", "51.75");
        exp_geoip_attr.insert("longitude", "-1.25");
        exp_geoip_attr.insert("postal_code", "OX1");

        for field in exp_geoip_attr.keys() {
            let k = Atom::from(format!("geo.{}", field).to_string());
            let geodata = new_event.as_log().get(&k).unwrap().to_string_lossy();
            assert_eq!(&geodata, exp_geoip_attr.get(field).expect("field exists"));
        }
    }

    #[test]
    fn geoip_city_lookup_partial_results() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "67.43.156.9", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();
        let reader = maxminddb::Reader::open_readfile("tests/data/GeoIP2-City-Test.mmdb").unwrap();

        let mut augment = Geoip::new(reader, Atom::from("remote_addr"), "geo".to_string());
        let new_event = augment.transform(event).unwrap();

        let mut exp_geoip_attr = HashMap::new();
        exp_geoip_attr.insert("city_name", "");
        exp_geoip_attr.insert("country_code", "BT");
        exp_geoip_attr.insert("continent_code", "AS");
        exp_geoip_attr.insert("timezone", "Asia/Thimphu");
        exp_geoip_attr.insert("latitude", "27.5");
        exp_geoip_attr.insert("longitude", "90.5");
        exp_geoip_attr.insert("postal_code", "");

        for field in exp_geoip_attr.keys() {
            let k = Atom::from(format!("geo.{}", field).to_string());
            let geodata = new_event.as_log().get(&k).unwrap().to_string_lossy();
            assert_eq!(&geodata, exp_geoip_attr.get(field).expect("field exists"));
        }
    }

    #[test]
    fn geoip_city_lookup_no_results() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "10.1.12.1", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();
        let reader = maxminddb::Reader::open_readfile("tests/data/GeoIP2-City-Test.mmdb").unwrap();

        let mut augment = Geoip::new(reader, Atom::from("remote_addr"), "geo".to_string());
        let new_event = augment.transform(event).unwrap();

        let mut exp_geoip_attr = HashMap::new();
        exp_geoip_attr.insert("city_name", "");
        exp_geoip_attr.insert("country_code", "");
        exp_geoip_attr.insert("continent_code", "");
        exp_geoip_attr.insert("timezone", "");
        exp_geoip_attr.insert("latitude", "");
        exp_geoip_attr.insert("longitude", "");
        exp_geoip_attr.insert("postal_code", "");

        for field in exp_geoip_attr.keys() {
            let k = Atom::from(format!("geo.{}", field).to_string());
            let geodata = new_event.as_log().get(&k).unwrap().to_string_lossy();
            assert_eq!(&geodata, exp_geoip_attr.get(field).expect("fields exists"));
        }
    }

    #[test]
    fn geoip_isp_lookup_success() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "208.192.1.2", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();
        let reader = maxminddb::Reader::open_readfile("tests/data/GeoIP2-ISP-Test.mmdb").unwrap();

        let mut augment = Geoip::new(reader, Atom::from("remote_addr"), "geo".to_string());
        let new_event = augment.transform(event).unwrap();

        let mut exp_geoip_attr = HashMap::new();
        exp_geoip_attr.insert("autonomous_system_number", "701");
        exp_geoip_attr.insert(
            "autonomous_system_organization",
            "MCI Communications Services, Inc. d/b/a Verizon Business",
        );
        exp_geoip_attr.insert("isp", "Verizon Business");
        exp_geoip_attr.insert("organization", "Verizon Business");

        for field in exp_geoip_attr.keys() {
            let k = Atom::from(format!("geo.{}", field).to_string());
            let geodata = new_event.as_log().get(&k).unwrap().to_string_lossy();
            assert_eq!(&geodata, exp_geoip_attr.get(field).expect("field exists"));
        }
    }

    #[test]
    fn geoip_isp_lookup_partial_results() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "2600:7000::1", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();
        let reader = maxminddb::Reader::open_readfile("tests/data/GeoLite2-ASN-Test.mmdb").unwrap();

        let mut augment = Geoip::new(reader, Atom::from("remote_addr"), "geo".to_string());
        let new_event = augment.transform(event).unwrap();

        let mut exp_geoip_attr = HashMap::new();
        exp_geoip_attr.insert("autonomous_system_number", "6939");
        exp_geoip_attr.insert("autonomous_system_organization", "Hurricane Electric, Inc.");
        exp_geoip_attr.insert("isp", "");
        exp_geoip_attr.insert("organization", "");

        for field in exp_geoip_attr.keys() {
            let k = Atom::from(format!("geo.{}", field).to_string());
            let geodata = new_event.as_log().get(&k).unwrap().to_string_lossy();
            assert_eq!(&geodata, exp_geoip_attr.get(field).expect("field exists"));
        }
    }

    #[test]
    fn geoip_isp_lookup_no_results() {
        let mut parser = JsonParser::from(JsonParserConfig::default());
        let event = Event::from(r#"{"remote_addr": "10.1.12.1", "request_path": "foo/bar"}"#);
        let event = parser.transform(event).unwrap();
        let reader = maxminddb::Reader::open_readfile("tests/data/GeoLite2-ASN-Test.mmdb").unwrap();

        let mut augment = Geoip::new(reader, Atom::from("remote_addr"), "geo".to_string());
        let new_event = augment.transform(event).unwrap();

        let mut exp_geoip_attr = HashMap::new();
        exp_geoip_attr.insert("autonomous_system_number", "0");
        exp_geoip_attr.insert("autonomous_system_organization", "");
        exp_geoip_attr.insert("isp", "");
        exp_geoip_attr.insert("organization", "");

        for field in exp_geoip_attr.keys() {
            let k = Atom::from(format!("geo.{}", field).to_string());
            let geodata = new_event.as_log().get(&k).unwrap().to_string_lossy();
            assert_eq!(&geodata, exp_geoip_attr.get(field).expect("fields exists"));
        }
    }
}
