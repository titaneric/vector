mod config;
mod integration_tests;
mod request_builder;
mod service;
mod sink;

use crate::config::SinkDescription;

use config::KinesisSinkConfig;

inventory::submit! {
    SinkDescription::new::<KinesisSinkConfig>("aws_kinesis_streams")
}
