use crate::event::Event;
use crate::tls::{MaybeTlsIncoming, TlsConfig, TlsSettings};
use futures01::{sync::mpsc, Future, IntoFuture, Sink};
use serde::Serialize;
use std::error::Error;
use std::fmt::{self, Display};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use stream_cancel::Tripwire;
use warp::filters::{body::FullBody, BoxedFilter};
use warp::http::{HeaderMap, StatusCode};
use warp::{Filter, Rejection};

#[derive(Serialize, Debug)]
pub struct ErrorMessage {
    code: u16,
    message: String,
}
impl ErrorMessage {
    pub fn new(code: StatusCode, message: String) -> Self {
        ErrorMessage {
            code: code.as_u16(),
            message,
        }
    }
}
impl Error for ErrorMessage {}
impl Display for ErrorMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

pub trait HttpSource: Clone + Send + Sync + 'static {
    fn build_event(
        &self,
        body: FullBody,
        header_map: HeaderMap,
    ) -> Result<Vec<Event>, ErrorMessage>;

    fn run(
        self,
        address: SocketAddr,
        path: &'static str,
        tls: &Option<TlsConfig>,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<crate::sources::Source> {
        let (trigger, tripwire) = Tripwire::new();
        let trigger = Arc::new(Mutex::new(Some(trigger)));

        let mut filter: BoxedFilter<()> = warp::post2().boxed();
        if !path.is_empty() && path != "/" {
            for s in path.split('/') {
                filter = filter.and(warp::path(s)).boxed();
            }
        }
        let svc = filter
            .and(warp::path::end())
            .and(warp::header::headers_cloned())
            .and(warp::body::concat())
            .and_then(move |headers: HeaderMap, body| {
                let out = out.clone();
                let trigger = trigger.clone();
                info!("Handling http request: {:?}", headers);

                self.build_event(body, headers)
                    .map_err(warp::reject::custom)
                    .into_future()
                    .and_then(|events| {
                        out.send_all(futures01::stream::iter_ok(events)).map_err(
                            move |e: mpsc::SendError<Event>| {
                                //can only fail if receiving end disconnected, so shut down and make some error logs
                                error!("Failed to forward events, downstream is closed");
                                error!("Tried to send the following event: {:?}", e);
                                error!("Shutting down");

                                trigger.try_lock().ok().take().map(drop); // shut down the http server if someone hasn't already
                                warp::reject::custom("shutting down")
                            },
                        )
                    })
                    .map(|_| warp::reply())
            });

        let ping = warp::get2().and(warp::path("ping")).map(|| "pong");
        let routes = svc.or(ping).recover(|r: Rejection| {
            if let Some(e_msg) = r.find_cause::<ErrorMessage>() {
                let json = warp::reply::json(e_msg);
                Ok(warp::reply::with_status(
                    json,
                    StatusCode::from_u16(e_msg.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                ))
            } else {
                //other internal error - will return 500 internal server error
                Err(r)
            }
            .into_future()
        });

        info!(message = "building http server", addr = %address);

        let tls = TlsSettings::from_config(tls, true)?;
        let incoming = MaybeTlsIncoming::bind(&address, tls)?;

        let server = warp::serve(routes).serve_incoming_with_graceful_shutdown(incoming, tripwire);

        Ok(Box::new(server))
    }
}
