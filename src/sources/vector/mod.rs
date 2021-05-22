pub mod v1;
pub mod v2;

use crate::config::{
    DataType, GenerateConfig, Resource, SourceConfig, SourceContext, SourceDescription,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
enum V1 {
    #[serde(rename = "1")]
    V1,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfigV1 {
    version: Option<V1>,
    #[serde(flatten)]
    config: v1::VectorConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum V2 {
    #[serde(rename = "2")]
    V2,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfigV2 {
    version: V2,
    #[serde(flatten)]
    config: v2::VectorConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum VectorConfig {
    V1(VectorConfigV1),
    V2(VectorConfigV2),
}

inventory::submit! {
    SourceDescription::new::<VectorConfig>("vector")
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        v2::VectorConfig::generate_config()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "vector")]
impl SourceConfig for VectorConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        match self {
            VectorConfig::V1(v1) => v1.config.build(cx).await,
            VectorConfig::V2(v2) => v2.config.build(cx).await,
        }
    }

    fn output_type(&self) -> DataType {
        match self {
            VectorConfig::V1(v1) => v1.config.output_type(),
            VectorConfig::V2(v2) => v2.config.output_type(),
        }
    }

    fn source_type(&self) -> &'static str {
        match self {
            VectorConfig::V1(v1) => v1.config.source_type(),
            VectorConfig::V2(v2) => v2.config.source_type(),
        }
    }
    fn resources(&self) -> Vec<Resource> {
        match self {
            VectorConfig::V1(v1) => v1.config.resources(),
            VectorConfig::V2(v2) => v2.config.resources(),
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::VectorConfig>();
    }
}
