use crate::config::{DataType, TransformConfig, TransformContext};
use crate::event::Event;
use crate::transforms::{FunctionTransform, Transform};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Noop;

#[async_trait::async_trait]
#[typetag::serde(name = "noop")]
impl TransformConfig for Noop {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(self.clone()))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn transform_type(&self) -> &'static str {
        "noop"
    }
}

impl FunctionTransform for Noop {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        output.push(event);
    }
}
