use crate::config::ComponentKey;
use crate::event::Event;
use futures::{channel::mpsc, future, stream::Fuse, Sink, Stream, StreamExt};
use std::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

pub type RouterSink = Box<dyn Sink<Event, Error = ()> + 'static + Send>;

pub enum ControlMessage {
    Add(ComponentKey, RouterSink),
    Remove(ComponentKey),
    /// Will stop accepting events until Some with given id is replaced.
    Replace(ComponentKey, Option<RouterSink>),
}

impl fmt::Debug for ControlMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ControlMessage::")?;
        match self {
            Self::Add(id, _) => write!(f, "Add({:?})", id),
            Self::Remove(id) => write!(f, "Remove({:?})", id),
            Self::Replace(id, _) => write!(f, "Replace({:?})", id),
        }
    }
}

pub type ControlChannel = mpsc::UnboundedSender<ControlMessage>;

pub struct Fanout {
    sinks: Vec<(ComponentKey, Option<Pin<RouterSink>>)>,
    i: usize,
    control_channel: Fuse<mpsc::UnboundedReceiver<ControlMessage>>,
}

impl Fanout {
    pub fn new() -> (Self, ControlChannel) {
        let (control_tx, control_rx) = mpsc::unbounded();

        let fanout = Self {
            sinks: vec![],
            i: 0,
            control_channel: control_rx.fuse(),
        };

        (fanout, control_tx)
    }

    /// Add a new sink as an output.
    ///
    /// # Panics
    /// Function will panic if a sink with the same ID is already present.
    pub fn add(&mut self, id: ComponentKey, sink: RouterSink) {
        assert!(
            !self.sinks.iter().any(|(n, _)| n == &id),
            "Duplicate output id in fanout"
        );

        self.sinks.push((id, Some(sink.into())));
    }

    fn remove(&mut self, id: &ComponentKey) {
        let i = self.sinks.iter().position(|(n, _)| n == id);
        let i = i.expect("Didn't find output in fanout");

        let (_id, removed) = self.sinks.remove(i);

        if let Some(mut removed) = removed {
            tokio::spawn(future::poll_fn(move |cx| removed.as_mut().poll_close(cx)));
        }

        if self.i > i {
            self.i -= 1;
        }
    }

    fn replace(&mut self, id: &ComponentKey, sink: Option<RouterSink>) {
        if let Some((_, existing)) = self.sinks.iter_mut().find(|(n, _)| n == id) {
            *existing = sink.map(Into::into);
        } else {
            panic!("Tried to replace a sink that's not already present");
        }
    }

    pub fn process_control_messages(&mut self, cx: &mut Context<'_>) {
        while let Poll::Ready(Some(message)) = Pin::new(&mut self.control_channel).poll_next(cx) {
            match message {
                ControlMessage::Add(id, sink) => self.add(id, sink),
                ControlMessage::Remove(id) => self.remove(&id),
                ControlMessage::Replace(id, sink) => self.replace(&id, sink),
            }
        }
    }

    fn handle_sink_error(&mut self, index: usize) -> Result<(), ()> {
        // If there's only one sink, propagate the error to the source ASAP
        // so it stops reading from its input. If there are multiple sinks,
        // keep pushing to the non-errored ones (while the errored sink
        // triggers a more graceful shutdown).
        if self.sinks.len() == 1 {
            Err(())
        } else {
            self.sinks.remove(index);
            Ok(())
        }
    }

    fn poll_sinks<F>(&mut self, cx: &mut Context<'_>, poll: F) -> Poll<Result<(), ()>>
    where
        F: Fn(&mut Pin<RouterSink>, &mut Context<'_>) -> Poll<Result<(), ()>>,
    {
        self.process_control_messages(cx);

        let mut poll_result = Poll::Ready(Ok(()));

        let mut i = 0;
        while let Some((_, sink)) = self.sinks.get_mut(i) {
            if let Some(sink) = sink {
                match poll(sink, cx) {
                    Poll::Pending => poll_result = Poll::Pending,
                    Poll::Ready(Ok(())) => (),
                    Poll::Ready(Err(())) => {
                        self.handle_sink_error(i)?;
                        continue;
                    }
                }
            }
            i += 1;
        }

        poll_result
    }
}

impl Sink<Event> for Fanout {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), ()>> {
        let this = self.get_mut();

        this.process_control_messages(cx);

        while let Some((_, sink)) = this.sinks.get_mut(this.i) {
            match sink.as_mut() {
                Some(sink) => match sink.as_mut().poll_ready(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Ok(())) => this.i += 1,
                    Poll::Ready(Err(())) => this.handle_sink_error(this.i)?,
                },
                // process_control_messages ended because control channel returned
                // Pending so it's fine to return Pending here since the control
                // channel will notify current task when it receives a message.
                None => return Poll::Pending,
            }
        }

        this.i = 0;

        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Event) -> Result<(), ()> {
        let mut i = 1;
        while let Some((_, sink)) = self.sinks.get_mut(i) {
            if let Some(sink) = sink.as_mut() {
                if sink.as_mut().start_send(item.clone()).is_err() {
                    self.handle_sink_error(i)?;
                    continue;
                }
            }
            i += 1;
        }

        if let Some((_, sink)) = self.sinks.first_mut() {
            if let Some(sink) = sink.as_mut() {
                if sink.as_mut().start_send(item).is_err() {
                    self.handle_sink_error(0)?;
                }
            }
        }

        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), ()>> {
        self.poll_sinks(cx, |sink, cx| sink.as_mut().poll_flush(cx))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), ()>> {
        self.poll_sinks(cx, |sink, cx| sink.as_mut().poll_close(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::{ControlMessage, Fanout};
    use crate::{config::ComponentKey, event::Event, test_util::collect_ready};
    use futures::{channel::mpsc, stream, FutureExt, Sink, SinkExt, StreamExt};
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn fanout_writes_to_all() {
        let (tx_a, rx_a) = mpsc::unbounded();
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::unbounded();
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));

        let (mut fanout, _fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), tx_a);
        fanout.add(ComponentKey::from("b"), tx_b);

        let recs = make_events(2);
        let send = stream::iter(recs.clone()).map(Ok).forward(fanout);
        send.await.unwrap();

        assert_eq!(collect_ready(rx_a), recs);
        assert_eq!(collect_ready(rx_b), recs);
    }

    #[tokio::test]
    async fn fanout_notready() {
        let (tx_a, rx_a) = mpsc::channel(1);
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::channel(0);
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));
        let (tx_c, rx_c) = mpsc::channel(1);
        let tx_c = Box::new(tx_c.sink_map_err(|_| unreachable!()));

        let (mut fanout, _fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), tx_a);
        fanout.add(ComponentKey::from("b"), tx_b);
        fanout.add(ComponentKey::from("c"), tx_c);

        let recs = make_events(3);
        let send = stream::iter(recs.clone()).map(Ok).forward(fanout);
        tokio::spawn(send);

        sleep(Duration::from_millis(50)).await;
        // The send_all task will be blocked on sending rec1 because of b right now.

        let collect_a = tokio::spawn(rx_a.collect::<Vec<_>>());
        let collect_b = tokio::spawn(rx_b.collect::<Vec<_>>());
        let collect_c = tokio::spawn(rx_c.collect::<Vec<_>>());

        assert_eq!(collect_a.await.unwrap(), recs);
        assert_eq!(collect_b.await.unwrap(), recs);
        assert_eq!(collect_c.await.unwrap(), recs);
    }

    #[tokio::test]
    async fn fanout_grow() {
        let (tx_a, rx_a) = mpsc::unbounded();
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::unbounded();
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));

        let (mut fanout, _fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), tx_a);
        fanout.add(ComponentKey::from("b"), tx_b);

        let recs = make_events(3);

        fanout.send(recs[0].clone()).await.unwrap();
        fanout.send(recs[1].clone()).await.unwrap();

        let (tx_c, rx_c) = mpsc::unbounded();
        let tx_c = Box::new(tx_c.sink_map_err(|_| unreachable!()));
        fanout.add(ComponentKey::from("c"), tx_c);

        fanout.send(recs[2].clone()).await.unwrap();

        assert_eq!(collect_ready(rx_a), recs);
        assert_eq!(collect_ready(rx_b), recs);
        assert_eq!(collect_ready(rx_c), &recs[2..]);
    }

    #[tokio::test]
    async fn fanout_shrink() {
        let (tx_a, rx_a) = mpsc::unbounded();
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::unbounded();
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));

        let (mut fanout, mut fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), tx_a);
        fanout.add(ComponentKey::from("b"), tx_b);

        let recs = make_events(3);

        fanout.send(recs[0].clone()).await.unwrap();
        fanout.send(recs[1].clone()).await.unwrap();

        fanout_control
            .send(ControlMessage::Remove(ComponentKey::from("b")))
            .await
            .unwrap();

        fanout.send(recs[2].clone()).await.unwrap();

        assert_eq!(collect_ready(rx_a), recs);
        assert_eq!(collect_ready(rx_b), &recs[..2]);
    }

    #[tokio::test]
    async fn fanout_shrink_after_notready() {
        let (tx_a, rx_a) = mpsc::channel(1);
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::channel(0);
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));
        let (tx_c, rx_c) = mpsc::channel(1);
        let tx_c = Box::new(tx_c.sink_map_err(|_| unreachable!()));

        let (mut fanout, mut fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), tx_a);
        fanout.add(ComponentKey::from("b"), tx_b);
        fanout.add(ComponentKey::from("c"), tx_c);

        let recs = make_events(3);
        let send = stream::iter(recs.clone()).map(Ok).forward(fanout);
        tokio::spawn(send);

        sleep(Duration::from_millis(50)).await;
        // The send_all task will be blocked on sending rec1 because of b right now.
        fanout_control
            .send(ControlMessage::Remove(ComponentKey::from("c")))
            .await
            .unwrap();

        let collect_a = tokio::spawn(rx_a.collect::<Vec<_>>());
        let collect_b = tokio::spawn(rx_b.collect::<Vec<_>>());
        let collect_c = tokio::spawn(rx_c.collect::<Vec<_>>());

        assert_eq!(collect_a.await.unwrap(), recs);
        assert_eq!(collect_b.await.unwrap(), recs);
        assert_eq!(collect_c.await.unwrap(), &recs[..1]);
    }

    #[tokio::test]
    async fn fanout_shrink_at_notready() {
        let (tx_a, rx_a) = mpsc::channel(1);
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::channel(0);
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));
        let (tx_c, rx_c) = mpsc::channel(1);
        let tx_c = Box::new(tx_c.sink_map_err(|_| unreachable!()));

        let (mut fanout, mut fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), tx_a);
        fanout.add(ComponentKey::from("b"), tx_b);
        fanout.add(ComponentKey::from("c"), tx_c);

        let recs = make_events(3);
        let send = stream::iter(recs.clone()).map(Ok).forward(fanout);
        tokio::spawn(send);

        sleep(Duration::from_millis(50)).await;
        // The send_all task will be blocked on sending rec1 because of b right now.
        fanout_control
            .send(ControlMessage::Remove(ComponentKey::from("b")))
            .await
            .unwrap();

        let collect_a = tokio::spawn(rx_a.collect::<Vec<_>>());
        let collect_b = tokio::spawn(rx_b.collect::<Vec<_>>());
        let collect_c = tokio::spawn(rx_c.collect::<Vec<_>>());

        assert_eq!(collect_a.await.unwrap(), recs);
        assert_eq!(collect_b.await.unwrap(), &recs[..1]);
        assert_eq!(collect_c.await.unwrap(), recs);
    }

    #[tokio::test]
    async fn fanout_shrink_before_notready() {
        let (tx_a, rx_a) = mpsc::channel(1);
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::channel(0);
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));
        let (tx_c, rx_c) = mpsc::channel(1);
        let tx_c = Box::new(tx_c.sink_map_err(|_| unreachable!()));

        let (mut fanout, mut fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), tx_a);
        fanout.add(ComponentKey::from("b"), tx_b);
        fanout.add(ComponentKey::from("c"), tx_c);

        let recs = make_events(3);
        let send = stream::iter(recs.clone()).map(Ok).forward(fanout);
        tokio::spawn(send);

        sleep(Duration::from_millis(50)).await;
        // The send_all task will be blocked on sending rec1 because of b right now.

        fanout_control
            .send(ControlMessage::Remove(ComponentKey::from("a")))
            .await
            .unwrap();

        let collect_a = tokio::spawn(rx_a.collect::<Vec<_>>());
        let collect_b = tokio::spawn(rx_b.collect::<Vec<_>>());
        let collect_c = tokio::spawn(rx_c.collect::<Vec<_>>());

        assert_eq!(collect_a.await.unwrap(), &recs[..1]);
        assert_eq!(collect_b.await.unwrap(), recs);
        assert_eq!(collect_c.await.unwrap(), recs);
    }

    #[tokio::test]
    async fn fanout_no_sinks() {
        let (mut fanout, _fanout_control) = Fanout::new();

        let recs = make_events(2);

        fanout.send(recs[0].clone()).await.unwrap();
        fanout.send(recs[1].clone()).await.unwrap();
    }

    #[tokio::test]
    async fn fanout_replace() {
        let (tx_a1, rx_a1) = mpsc::unbounded();
        let tx_a1 = Box::new(tx_a1.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::unbounded();
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));

        let (mut fanout, _fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), tx_a1);
        fanout.add(ComponentKey::from("b"), tx_b);

        let recs = make_events(3);

        fanout.send(recs[0].clone()).await.unwrap();
        fanout.send(recs[1].clone()).await.unwrap();

        let (tx_a2, rx_a2) = mpsc::unbounded();
        let tx_a2 = Box::new(tx_a2.sink_map_err(|_| unreachable!()));
        fanout.replace(&ComponentKey::from("a"), Some(tx_a2));

        fanout.send(recs[2].clone()).await.unwrap();

        assert_eq!(collect_ready(rx_a1), &recs[..2]);
        assert_eq!(collect_ready(rx_b), recs);
        assert_eq!(collect_ready(rx_a2), &recs[2..]);
    }

    #[tokio::test]
    async fn fanout_wait() {
        let (tx_a1, rx_a1) = mpsc::unbounded();
        let tx_a1 = Box::new(tx_a1.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::unbounded();
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));

        let (mut fanout, mut fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), tx_a1);
        fanout.add(ComponentKey::from("b"), tx_b);

        let recs = make_events(3);

        fanout.send(recs[0].clone()).await.unwrap();
        fanout.send(recs[1].clone()).await.unwrap();

        let (tx_a2, rx_a2) = mpsc::unbounded();
        let tx_a2 = Box::new(tx_a2.sink_map_err(|_| unreachable!()));
        fanout.replace(&ComponentKey::from("a"), None);

        futures::join!(
            async {
                sleep(Duration::from_millis(100)).await;
                fanout_control
                    .send(ControlMessage::Replace(
                        ComponentKey::from("a"),
                        Some(tx_a2),
                    ))
                    .await
                    .unwrap();
            },
            fanout.send(recs[2].clone()).map(|_| ())
        );

        assert_eq!(collect_ready(rx_a1), &recs[..2]);
        assert_eq!(collect_ready(rx_b), recs);
        assert_eq!(collect_ready(rx_a2), &recs[2..]);
    }

    #[tokio::test]
    async fn fanout_error_poll_first() {
        fanout_error(&[Some(ErrorWhen::Poll), None, None]).await;
    }

    #[tokio::test]
    async fn fanout_error_poll_middle() {
        fanout_error(&[None, Some(ErrorWhen::Poll), None]).await;
    }

    #[tokio::test]
    async fn fanout_error_poll_last() {
        fanout_error(&[None, None, Some(ErrorWhen::Poll)]).await;
    }

    #[tokio::test]
    async fn fanout_error_poll_not_middle() {
        fanout_error(&[Some(ErrorWhen::Poll), None, Some(ErrorWhen::Poll)]).await;
    }

    #[tokio::test]
    async fn fanout_error_send_first() {
        fanout_error(&[Some(ErrorWhen::Send), None, None]).await;
    }

    #[tokio::test]
    async fn fanout_error_send_middle() {
        fanout_error(&[None, Some(ErrorWhen::Send), None]).await;
    }

    #[tokio::test]
    async fn fanout_error_send_last() {
        fanout_error(&[None, None, Some(ErrorWhen::Send)]).await;
    }

    #[tokio::test]
    async fn fanout_error_send_not_middle() {
        fanout_error(&[Some(ErrorWhen::Send), None, Some(ErrorWhen::Send)]).await;
    }

    async fn fanout_error(modes: &[Option<ErrorWhen>]) {
        let (mut fanout, _fanout_control) = Fanout::new();
        let mut rx_channels = vec![];

        for (i, mode) in modes.iter().enumerate() {
            let id = ComponentKey::from(format!("{}", i));
            if let Some(when) = *mode {
                let tx = AlwaysErrors { when };
                let tx = Box::new(tx.sink_map_err(|_| ()));
                fanout.add(id, tx);
            } else {
                let (tx, rx) = mpsc::channel(0);
                let tx = Box::new(tx.sink_map_err(|_| unreachable!()));
                fanout.add(id, tx);
                rx_channels.push(rx);
            }
        }

        let recs = make_events(3);
        let send = stream::iter(recs.clone()).map(Ok).forward(fanout);
        tokio::spawn(send);

        sleep(Duration::from_millis(50)).await;

        // Start collecting from all at once
        let collectors = rx_channels
            .into_iter()
            .map(|rx| tokio::spawn(rx.collect::<Vec<_>>()))
            .collect::<Vec<_>>();

        for collect in collectors {
            assert_eq!(collect.await.unwrap(), recs);
        }
    }

    #[derive(Clone, Copy)]
    enum ErrorWhen {
        Send,
        Poll,
    }

    struct AlwaysErrors {
        when: ErrorWhen,
    }

    impl Sink<Event> for AlwaysErrors {
        type Error = crate::Error;

        fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(match self.when {
                ErrorWhen::Poll => Err("Something failed".into()),
                _ => Ok(()),
            })
        }

        fn start_send(self: Pin<&mut Self>, _: Event) -> Result<(), Self::Error> {
            match self.when {
                ErrorWhen::Poll => Err("Something failed".into()),
                _ => Ok(()),
            }
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(match self.when {
                ErrorWhen::Poll => Err("Something failed".into()),
                _ => Ok(()),
            })
        }

        fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(match self.when {
                ErrorWhen::Poll => Err("Something failed".into()),
                _ => Ok(()),
            })
        }
    }

    fn make_events(count: usize) -> Vec<Event> {
        (0..count)
            .map(|i| Event::from(format!("line {}", i)))
            .collect()
    }
}
