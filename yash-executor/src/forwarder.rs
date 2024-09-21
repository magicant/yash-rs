// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! Utilities for forwarding the result of a future to another future
//!
//! The [`forwarder`] function creates a pair of [`Sender`] and [`Receiver`] that
//! can be used to forward the result of a future to another future. The sender
//! half is used to send the result, and the receiver half is used to receive the
//! result.
//!
//! ```
//! # use yash_executor::forwarder::*;
//! let (sender, receiver) = forwarder::<u32>();
//!
//! // The result is not yet available
//! assert_eq!(receiver.try_receive(), Err(TryReceiveError::NotSent));
//!
//! // Send the result
//! sender.send(42).unwrap();
//!
//! // The result is now available
//! assert_eq!(receiver.try_receive(), Ok(42));
//! ```
//!
//! If the `Sender` is dropped before sending the result, the `Receiver` will
//! never receive the result. If the `Receiver` is dropped before receiving the
//! result, the `Sender` will not be able to send the result, but it does not
//! otherwise affect the task that produces the result.

use alloc::rc::{Rc, Weak};
use core::cell::RefCell;
use core::fmt::Display;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

/// State shared between the sender and receiver
#[derive(Debug)]
enum Relay<T> {
    /// The result has not been computed yet, and the receiver has not been polled.
    Pending,
    /// The result has not been computed yet, and the receiver has been polled.
    Polled(Waker),
    /// The result has been computed, but the receiver has not received it yet.
    Computed(T),
    /// The receiver has received the result.
    Done,
}

/// Sender half of the forwarder
///
/// See the [module-level documentation](self) for more information.
#[derive(Debug)]
pub struct Sender<T> {
    relay: Weak<RefCell<Relay<T>>>,
}

/// Receiver half of the forwarder
///
/// Call [`try_receive`](Self::try_receive) to examine if the result has been
/// sent from the sender. `Receiver` also implements the `Future` trait, so you
/// can use it in an async block or function to receive the result
/// asynchronously.
///
/// See also the [module-level documentation](self) for more information.
#[derive(Debug)]
pub struct Receiver<T> {
    relay: Rc<RefCell<Relay<T>>>,
}

/// Creates a new forwarder.
#[must_use]
pub fn forwarder<T>() -> (Sender<T>, Receiver<T>) {
    let relay = Rc::new(RefCell::new(Relay::Pending));
    let sender = Sender {
        relay: Rc::downgrade(&relay),
    };
    let receiver = Receiver { relay };
    (sender, receiver)
}

/// Error returned when receiving a value fails
///
/// This error may be returned from the [`Receiver::try_receive`] method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TryReceiveError {
    /// The sender has been dropped, which means the receiver will never receive
    /// the value.
    SenderDropped,
    /// The value has not been sent yet.
    NotSent,
    /// The value has already been received.
    AlreadyReceived,
}

impl<T> Sender<T> {
    /// Sends a value to the receiver.
    ///
    /// The value is sent to the receiver. If the the receiver has been dropped,
    /// the value is returned back to the caller.
    ///
    /// This method consumes the sender, ensuring that the value is sent at most
    /// once for each sender-receiver pair.
    pub fn send(self, value: T) -> Result<(), T> {
        let Some(relay) = self.relay.upgrade() else {
            return Err(value);
        };

        let relay = &mut *relay.borrow_mut();
        match core::mem::replace(relay, Relay::Computed(value)) {
            Relay::Pending => Ok(()),
            Relay::Polled(waker) => {
                waker.wake();
                Ok(())
            }
            // We can send only once, so these cases are impossible
            Relay::Computed(_) | Relay::Done => unreachable!(),
        }
    }
}

impl<T> Receiver<T> {
    /// Receives a value from the sender.
    ///
    /// This method is similar to [`poll`](Self::poll), but it does not require
    /// a `Context` argument. If the value has not been sent yet, this method
    /// returns `Err(TryReceiveError::NotSent)`.
    pub fn try_receive(&self) -> Result<T, TryReceiveError> {
        let relay = &mut *self.relay.borrow_mut();
        match relay {
            Relay::Pending | Relay::Polled(_) => {
                if Rc::weak_count(&self.relay) == 0 {
                    Err(TryReceiveError::SenderDropped)
                } else {
                    Err(TryReceiveError::NotSent)
                }
            }

            Relay::Computed(_) => {
                let Relay::Computed(value) = core::mem::replace(relay, Relay::Done) else {
                    unreachable!()
                };
                Ok(value)
            }

            Relay::Done => Err(TryReceiveError::AlreadyReceived),
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = T;

    /// Polls the receiver to receive the value.
    ///
    /// This method is similar to [`try_receive`](Self::try_receive), but it
    /// requires a `Context` argument. If the value has not been sent yet, this
    /// method returns `Poll::Pending` and stores the `Waker` from the `Context`
    /// for waking up the current task when the value is sent.
    ///
    /// This method should not be called after the value has been received.
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<T> {
        let relay = &mut *self.relay.borrow_mut();
        match relay {
            Relay::Pending | Relay::Polled(_) => {
                *relay = Relay::Polled(context.waker().clone());
                Poll::Pending
            }

            Relay::Computed(_) => {
                let Relay::Computed(value) = core::mem::replace(relay, Relay::Done) else {
                    unreachable!()
                };
                Poll::Ready(value)
            }

            Relay::Done => panic!("Receiver polled after receiving the value"),
        }
    }
}

impl Display for TryReceiveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TryReceiveError::SenderDropped => "sender already dropped".fmt(f),
            TryReceiveError::NotSent => "result not sent yet".fmt(f),
            TryReceiveError::AlreadyReceived => "result already received".fmt(f),
        }
    }
}

// TODO Bump MSRV to 1.81.0 to impl core::error::Error for TryReceiveError
