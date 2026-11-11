//! What the unit tests share: running a future without a runtime, a [`Transport`] that answers
//! from a recording, an [`Http`] wire that answers from a queue, recorded venue answers, and the
//! test keys. Nothing leaves the process.

use std::{
    cell::RefCell,
    collections::VecDeque,
    future::{Future, ready},
    pin::pin,
    task::{Context, Poll, Waker},
};

use crate::client::{Call, Http, Request, Response, Transport, TransportError};

/// Runs a future to its end on this thread. The fakes here never wait, so each future is ready
/// on its first poll.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

/// A [`Transport`] that answers every call the same way and keeps each one.
pub(crate) struct Recorded {
    answer: Result<String, TransportError>,
    sent: RefCell<Vec<Call>>,
}

impl Recorded {
    pub(crate) fn answering(text: &str) -> Recorded {
        Recorded { answer: Ok(text.to_owned()), sent: RefCell::default() }
    }

    pub(crate) fn failing(error: TransportError) -> Recorded {
        Recorded { answer: Err(error), sent: RefCell::default() }
    }

    /// Each call, in order.
    pub(crate) fn sent(&self) -> Vec<Call> {
        self.sent.borrow().clone()
    }

    /// The one call made.
    pub(crate) fn only(&self) -> Call {
        let sent = self.sent();
        assert_eq!(sent.len(), 1, "one call: {sent:?}");
        sent.into_iter().next().unwrap_or_else(|| Call::get("none"))
    }
}

impl Transport for Recorded {
    fn call(&self, call: Call) -> impl Future<Output = Result<String, TransportError>> {
        self.sent.borrow_mut().push(call);
        ready(self.answer.clone())
    }
}

/// An [`Http`] wire that answers from a queue, then with its fallback, and keeps every request.
pub(crate) struct Wire {
    answers: RefCell<VecDeque<Result<Response, TransportError>>>,
    otherwise: Result<Response, TransportError>,
    sent: RefCell<Vec<Request>>,
}

impl Wire {
    /// Answers with `answers` in turn, then as unreachable.
    pub(crate) fn answering(answers: impl IntoIterator<Item = Result<Response, TransportError>>) -> Wire {
        Wire {
            answers: RefCell::new(answers.into_iter().collect()),
            otherwise: Err(TransportError::Unreachable("dry".to_owned())),
            sent: RefCell::default(),
        }
    }

    /// Answers every request `200` with `text`.
    pub(crate) fn always(text: &str) -> Wire {
        Wire::answering([]).then(ok(text))
    }

    /// The same wire, answering `otherwise` once its queue is empty.
    pub(crate) fn then(self, otherwise: Result<Response, TransportError>) -> Wire {
        Wire { otherwise, ..self }
    }

    /// Every request, in order.
    pub(crate) fn sent(&self) -> Vec<Request> {
        self.sent.borrow().clone()
    }

    /// Every request as its URL and its body's text.
    pub(crate) fn bodies(&self) -> Vec<(String, String)> {
        self.sent()
            .into_iter()
            .map(|request| (request.url, String::from_utf8_lossy(&request.body).into_owned()))
            .collect()
    }
}

impl Http for Wire {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>> {
        self.sent.borrow_mut().push(request);
        ready(self.answers.borrow_mut().pop_front().unwrap_or_else(|| self.otherwise.clone()))
    }
}

/// A `200` with `text` as its body.
pub(crate) fn ok(text: &str) -> Result<Response, TransportError> {
    status(200, text)
}

/// An answer with `status` and `text` as its body.
pub(crate) fn status(status: u16, text: &str) -> Result<Response, TransportError> {
    Ok(Response { status, headers: Vec::new(), body: text.as_bytes().to_vec() })
}

/// A fixed clock: 2026-09-21T22:13:20Z.
pub(crate) fn clock() -> u64 {
    1_790_000_000_000
}

/// The keys the tests sign with, derived from labels: never a real wallet. The same as the
/// globe.li vectors' (`vectors/aster_web.json`).
#[cfg(feature = "sign")]
pub(crate) fn main_key() -> crate::utils::sign::PrivateKey {
    let bytes = crate::utils::sign::keccak256(b"atlas aster main");
    crate::utils::sign::PrivateKey::from_bytes(&bytes).unwrap_or_else(|error| panic!("{error}"))
}

#[cfg(feature = "sign")]
pub(crate) fn agent() -> crate::utils::sign::Agent {
    let bytes = crate::utils::sign::keccak256(b"atlas aster agent");
    let key = crate::utils::sign::PrivateKey::from_bytes(&bytes).unwrap_or_else(|error| panic!("{error}"));
    crate::utils::sign::Agent::new(main_key().address(), key)
}

/// A client on a fake wire, signing as the test agent, its clock fixed.
#[cfg(feature = "sign")]
pub(crate) fn signing_client(wire: Wire) -> crate::client::Client<Wire, crate::utils::sign::Agent> {
    crate::client::Client::new(wire, clock, agent())
}
