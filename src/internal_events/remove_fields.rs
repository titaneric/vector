use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub(crate) struct RemoveFieldsFieldMissing<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for RemoveFieldsFieldMissing<'a> {
    fn emit_logs(&self) {
        debug!(message = "Field did not exist.", field = %self.field, internal_log_rate_secs = 30);
    }
}
