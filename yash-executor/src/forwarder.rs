// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

//! Utilities for forwarding the result of a future to another future
//!
//! The [`forwarder`] function creates a pair of [`Sender`] and [`Receiver`] that
//! can be used to forward the result of a future to another future. The sender
//! half is used to send the result, and the receiver half is used to receive the
//! result.

use alloc::rc::{Rc, Weak};
use core::cell::RefCell;
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
#[derive(Debug)]
pub struct Sender<T> {
    relay: Weak<RefCell<Relay<T>>>,
}

/// Receiver half of the forwarder
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

/// Error returned when sending a value fails
///
/// This error may be returned from the [`Sender::send`] method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SendError {
    /// The receiver has been dropped.
    ReceiverDropped,
    /// The value has already been sent.
    AlreadySent,
}

/// Error returned when receiving a value fails
///
/// This error may be returned from the [`Receiver::try_receive`] method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TryReceiveError {
    /// The sender has been dropped.
    SenderDropped,
    /// The value has not been sent yet.
    NotSent,
    /// The value has already been received.
    AlreadyReceived,
}

impl<T> Sender<T> {
    /// Sends a value to the receiver.
    pub fn send(&self, value: T) -> Result<(), (T, SendError)> {
        let Some(relay) = self.relay.upgrade() else {
            return Err((value, SendError::ReceiverDropped));
        };

        let relay = &mut *relay.borrow_mut();
        match relay {
            Relay::Pending => {
                *relay = Relay::Computed(value);
                Ok(())
            }

            Relay::Polled(_) => {
                let Relay::Polled(waker) = core::mem::replace(relay, Relay::Computed(value)) else {
                    unreachable!()
                };
                waker.wake();
                Ok(())
            }

            Relay::Computed(_) | Relay::Done => Err((value, SendError::AlreadySent)),
        }
    }
}

impl<T> Receiver<T> {
    /// Receives a value from the sender.
    ///
    /// This method is similar to [`poll()`](Self::poll), but it does not
    /// require a `Context` argument. If the value has not been sent yet, this
    /// method returns `Err(TryReceiveError::NotSent)`.
    pub fn try_receive(&self) -> Result<T, TryReceiveError> {
        if Rc::weak_count(&self.relay) == 0 {
            return Err(TryReceiveError::SenderDropped);
        }

        let relay = &mut *self.relay.borrow_mut();
        match relay {
            Relay::Pending | Relay::Polled(_) => Err(TryReceiveError::NotSent),
            Relay::Done => Err(TryReceiveError::AlreadyReceived),

            Relay::Computed(_) => {
                let Relay::Computed(value) = core::mem::replace(relay, Relay::Done) else {
                    unreachable!()
                };
                Ok(value)
            }
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = T;

    /// Polls the receiver to receive the value.
    ///
    /// This method is similar to [`try_receive()`](Self::try_receive), but it
    /// requires a `Context` argument. If the value has not been sent yet, this
    /// method returns `Poll::Pending` and stores the `Waker` from the `Context`
    /// for waking up the task when the value is sent.
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
