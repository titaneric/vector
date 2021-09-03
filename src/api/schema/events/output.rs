use super::{
    log::Log,
    notification::{EventNotification, EventNotificationType},
};
use crate::api::tap::{TapNotification, TapPayload};

use async_graphql::Union;

#[derive(Union, Debug)]
/// An event or a notification
pub enum OutputEventsPayload {
    /// Log event
    Log(Log),

    // Notification
    Notification(EventNotification),
}

/// Convert an `api::TapPayload` to the equivalent GraphQL type.
impl From<TapPayload> for OutputEventsPayload {
    fn from(t: TapPayload) -> Self {
        match t {
            TapPayload::Log(component_id, ev) => Self::Log(Log::new(component_id, ev)),
            TapPayload::Notification(component_id, n) => match n {
                TapNotification::Matched => Self::Notification(EventNotification::new(
                    component_id,
                    EventNotificationType::Matched,
                )),
                TapNotification::NotMatched => Self::Notification(EventNotification::new(
                    component_id,
                    EventNotificationType::NotMatched,
                )),
            },
            _ => unreachable!("TODO: implement metrics"),
        }
    }
}
