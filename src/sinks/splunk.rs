use super::util;
use super::util::SinkExt;
use futures::{Future, Sink};
use hyper::{Request, Uri};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::record::Record;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct HecSinkConfig {
    pub token: String,
    pub host: String,
}

#[typetag::serde(name = "splunk_hec")]
impl crate::topology::config::SinkConfig for HecSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        Ok((
            hec(self.token.clone(), self.host.clone()),
            hec_healthcheck(self.token.clone(), self.host.clone()),
        ))
    }
}

pub fn hec(token: String, host: String) -> super::RouterSink {
    let sink = util::http::HttpSink::new()
        .with(move |body: Vec<u8>| {
            let uri = format!("{}/services/collector/event", host);
            let uri: Uri = uri.parse().unwrap();

            let request = Request::post(uri)
                .header("Content-Type", "application/json")
                .header("Content-Encoding", "gzip")
                .header("Authorization", format!("Splunk {}", token))
                .body(body.into())
                .unwrap();

            Ok(request)
        })
        .size_buffered(2 * 1024 * 1024, true)
        .with(move |record: Record| {
            let mut body = json!({
                "event": record.line,
                "fields": record.custom,
            });
            if let Some(host) = record.host {
                body["host"] = json!(host);
            }
            let body = serde_json::to_vec(&body).unwrap();
            Ok(body)
        });

    Box::new(sink)
}

pub fn hec_healthcheck(token: String, host: String) -> super::Healthcheck {
    use hyper::{Body, Client, Request};
    use hyper_tls::HttpsConnector;

    let uri = format!("{}/services/collector/health/1.0", host);
    let uri: Uri = uri.parse().unwrap();

    let request = Request::get(uri)
        .header("Authorization", format!("Splunk {}", token))
        .body(Body::empty())
        .unwrap();

    let https = HttpsConnector::new(4).expect("TLS initialization failed");
    let client = Client::builder().build(https);

    let healthcheck = client
        .request(request)
        .map_err(|err| err.to_string())
        .and_then(|response| {
            use hyper::StatusCode;

            match response.status() {
                StatusCode::OK => Ok(()),
                StatusCode::BAD_REQUEST => Err("Invalid HEC token".to_string()),
                StatusCode::SERVICE_UNAVAILABLE => {
                    Err("HEC is unhealthy, queues are full".to_string())
                }
                other => Err(format!("Unexpected status: {}", other)),
            }
        });

    Box::new(healthcheck)
}

#[cfg(test)]
mod tests {
    #![cfg(feature = "splunk-integration-tests")]

    use crate::{sinks, Record};
    use futures::Sink;
    use serde_json::Value as JsonValue;

    const USERNAME: &str = "admin";
    const PASSWORD: &str = "password";

    #[test]
    fn test_insert_message_into_splunk() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let sink = sinks::splunk::hec(get_token(), "http://localhost:8088".to_string());

        let message = random_string();
        let record = Record::from(message.clone());

        let pump = sink.send(record);

        rt.block_on(pump).unwrap();

        // It usually takes ~1 second for the event to show up in search, so poll until
        // we see it.
        let entry = (0..20)
            .find_map(|_| {
                recent_entries()
                    .into_iter()
                    .find(|entry| entry["_raw"].as_str().unwrap() == message)
                    .or_else(|| {
                        ::std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(message, entry["_raw"].as_str().unwrap());
    }

    #[test]
    fn test_insert_many() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let sink = sinks::splunk::hec(get_token(), "http://localhost:8088".to_string());

        let messages = (0..10).map(|_| random_string()).collect::<Vec<_>>();
        let records = messages
            .iter()
            .map(|l| Record::from(l.clone()))
            .collect::<Vec<_>>();

        let pump = sink.send_all(futures::stream::iter_ok(records));

        rt.block_on(pump).unwrap();

        let mut found_all = false;
        for _ in 0..20 {
            let entries = recent_entries();

            found_all = messages.iter().all(|message| {
                entries
                    .iter()
                    .any(|entry| entry["_raw"].as_str().unwrap() == message)
            });

            if found_all {
                break;
            }

            ::std::thread::sleep(std::time::Duration::from_millis(100));
        }

        assert!(found_all);
    }

    #[test]
    fn test_custom_fields() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let sink = sinks::splunk::hec(get_token(), "http://localhost:8088".to_string());

        let message = random_string();
        let mut record = Record::from(message.clone());
        record.custom.insert("asdf".into(), "hello".to_owned());

        let pump = sink.send(record);

        rt.block_on(pump).unwrap();

        let entry = (0..20)
            .find_map(|_| {
                recent_entries()
                    .into_iter()
                    .find(|entry| entry["_raw"].as_str().unwrap() == message)
                    .or_else(|| {
                        ::std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(message, entry["_raw"].as_str().unwrap());
        assert_eq!("hello", entry["asdf"].as_str().unwrap());
    }

    #[test]
    fn test_hostname() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let sink = sinks::splunk::hec(get_token(), "http://localhost:8088".to_string());

        let message = random_string();
        let mut record = Record::from(message.clone());
        record.custom.insert("asdf".into(), "hello".to_owned());
        record.host = Some("example.com:1234".to_owned());

        let pump = sink.send(record);

        rt.block_on(pump).unwrap();

        let entry = (0..20)
            .find_map(|_| {
                recent_entries()
                    .into_iter()
                    .find(|entry| entry["_raw"].as_str().unwrap() == message)
                    .or_else(|| {
                        ::std::thread::sleep(std::time::Duration::from_millis(100));
                        None
                    })
            })
            .expect("Didn't find event in Splunk");

        assert_eq!(message, entry["_raw"].as_str().unwrap());
        assert_eq!("hello", entry["asdf"].as_str().unwrap());
        assert_eq!("example.com:1234", entry["host"].as_str().unwrap());
    }

    #[test]
    fn test_healthcheck() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        // OK
        {
            let healthcheck =
                sinks::splunk::hec_healthcheck(get_token(), "http://localhost:8088".to_string());
            rt.block_on(healthcheck).unwrap();
        }

        // Server not listening at address
        {
            let healthcheck =
                sinks::splunk::hec_healthcheck(get_token(), "http://localhost:1111".to_string());
            assert_eq!(
                rt.block_on(healthcheck).unwrap_err(),
                "an error occurred trying to connect: Connection refused (os error 111)"
            );
        }

        // Invalid token
        // The HEC REST docs claim that the healthcheck endpoint will validate the auth token,
        // but my local testing server returns 200 even with a bad token.
        {
            // let healthcheck = sinks::splunk::hec_healthcheck("asdf".to_string(), "http://localhost:8088".to_string());
            // assert_eq!(rt.block_on(healthcheck).unwrap_err(), "Invalid HEC token");
        }

        // Unhealthy server
        {
            let healthcheck =
                sinks::splunk::hec_healthcheck(get_token(), "http://503.returnco.de".to_string());
            assert_eq!(
                rt.block_on(healthcheck).unwrap_err(),
                "HEC is unhealthy, queues are full"
            );
        }
    }

    fn recent_entries() -> Vec<JsonValue> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        // http://docs.splunk.com/Documentation/Splunk/7.2.1/RESTREF/RESTsearch#search.2Fjobs
        let mut res = client
            .post("https://localhost:8089/services/search/jobs?output_mode=json")
            .form(&[
                ("search", "search *"),
                ("exec_mode", "oneshot"),
                ("f", "_raw"),
                ("f", "asdf"),
                ("f", "host"),
            ])
            .basic_auth(USERNAME, Some(PASSWORD))
            .send()
            .unwrap();
        let json: JsonValue = res.json().unwrap();

        json["results"].as_array().unwrap().clone()
    }

    fn random_string() -> String {
        use rand::distributions::Alphanumeric;
        use rand::{thread_rng, Rng};

        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(100)
            .collect::<String>()
    }

    fn get_token() -> String {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        let mut res = client
            .get("https://localhost:8089/services/data/inputs/http?output_mode=json")
            .basic_auth(USERNAME, Some(PASSWORD))
            .send()
            .unwrap();

        let json: JsonValue = res.json().unwrap();
        let entries = json["entry"].as_array().unwrap().clone();

        if entries.is_empty() {
            // TODO: create one automatically
            panic!("You don't have any HTTP Event Collector inputs set up in Splunk");
        }

        let token = entries[0]["content"]["token"].as_str().unwrap().to_owned();

        token
    }

}
