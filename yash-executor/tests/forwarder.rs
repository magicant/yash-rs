// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki

use futures_task::noop_waker_ref;
use std::cell::Cell;
use std::future::poll_fn;
use std::future::Future as _;
use std::pin::pin;
use std::task::{Context, Poll};
use yash_executor::forwarder::*;
use yash_executor::Executor;

#[test]
fn send_and_try_receive() {
    let (sender, receiver) = forwarder::<u32>();
    let send_result = sender.send(42);
    assert_eq!(send_result, Ok(()));
    let receive_result = receiver.try_receive();
    assert_eq!(receive_result, Ok(42));
}

#[test]
fn send_to_dropped_receiver() {
    let (sender, _) = forwarder::<()>();
    let send_result = sender.send(());
    assert_eq!(send_result, Err(((), SendError::ReceiverDropped)));
}

#[test]
fn send_after_send() {
    let (sender, _receiver) = forwarder::<()>();
    sender.send(()).unwrap();
    let send_result = sender.send(());
    assert_eq!(send_result, Err(((), SendError::AlreadySent)));
}

#[test]
fn send_after_received() {
    let (sender, receiver) = forwarder::<()>();
    sender.send(()).unwrap();
    receiver.try_receive().unwrap();
    let send_result = sender.send(());
    assert_eq!(send_result, Err(((), SendError::AlreadySent)));
}

#[test]
fn try_receive_without_send() {
    let (_sender, receiver) = forwarder::<()>();
    let result = receiver.try_receive();
    assert_eq!(result, Err(TryReceiveError::NotSent));
}

#[test]
fn try_receive_from_dropped_sender() {
    let (_, receiver) = forwarder::<()>();
    let result = receiver.try_receive();
    assert_eq!(result, Err(TryReceiveError::SenderDropped));
}

#[test]
fn try_receive_after_received() {
    let (sender, receiver) = forwarder::<()>();
    sender.send(()).unwrap();
    receiver.try_receive().unwrap();
    let result = receiver.try_receive();
    assert_eq!(result, Err(TryReceiveError::AlreadyReceived));
}

#[test]
fn try_receive_does_not_modify_waker() {
    let received = Cell::new(false);
    let (sender, receiver) = forwarder::<()>();
    let mut receiver = pin!(receiver);
    let executor = Executor::new();
    unsafe {
        executor.spawn_pinned(Box::pin(poll_fn(|context| {
            let poll = receiver.as_mut().poll(context);
            if poll.is_pending() {
                assert_eq!(receiver.try_receive(), Err(TryReceiveError::NotSent));
            } else {
                received.set(true);
            }
            poll
        })));
    }

    // This polls the above task for the first time, in which the receiver is
    // not ready. The `try_receive` method is called but should not modify the
    // waker set in the `poll` call.
    executor.run_until_stalled();

    sender.send(()).unwrap();

    // This polls the above task for the second time, in which the receiver is
    // ready. The `received` flag should be set.
    executor.run_until_stalled();
    assert!(received.get());
}

#[test]
fn send_and_poll() {
    let received = Cell::new(false);
    let (sender, receiver) = forwarder::<u32>();
    sender.send(42).unwrap();
    let executor = Executor::new();
    unsafe {
        executor.spawn_pinned(Box::pin(async {
            let result = receiver.await;
            assert_eq!(result, 42);
            received.set(true);
        }));
    }
    executor.run_until_stalled();
    assert!(received.get());
}

#[test]
fn poll_and_send() {
    let received = Cell::new(false);
    let (sender, receiver) = forwarder::<u32>();
    let executor = Executor::new();
    unsafe {
        executor.spawn_pinned(Box::pin(async {
            let result = receiver.await;
            assert_eq!(result, 42);
            received.set(true);
        }));
    }
    executor.run_until_stalled();
    sender.send(42).unwrap();
    executor.run_until_stalled();
    assert!(received.get());
}

#[test]
fn second_poll_overwrites_waker() {
    // It is important that the relay keeps the waker from the last poll so that
    // it can wake the correct task.

    let received = Cell::new(false);
    let (sender, receiver) = forwarder::<()>();
    let mut receiver = pin!(receiver);
    let executor = Executor::new();
    unsafe {
        executor.spawn_pinned(Box::pin(poll_fn(|context| {
            let mut null_context = Context::from_waker(noop_waker_ref());
            match receiver.as_mut().poll(&mut null_context) {
                Poll::Pending => receiver.as_mut().poll(context),
                Poll::Ready(()) => {
                    received.set(true);
                    Poll::Ready(())
                }
            }
        })));
    }

    // This polls the above task for the first time, in which the receiver is
    // not ready. This involves two calls to `poll` on the receiver, and the
    // context from the second call is stored in the relay.
    executor.run_until_stalled();

    sender.send(()).unwrap();

    // This polls the above task for the second time, in which the receiver is
    // ready. The first call to `poll` on the receiver should return `Ready`,
    // so the `received` flag should be set.
    executor.run_until_stalled();
    assert!(received.get());
}

#[test]
#[should_panic = "polled after receiving"]
fn poll_after_received() {
    let (sender, mut receiver) = forwarder::<()>();
    sender.send(()).unwrap();
    let executor = Executor::new();
    unsafe {
        executor.spawn_pinned(Box::pin(async {
            (&mut receiver).await;
            receiver.await;
        }));
    }
    executor.run_until_stalled();
}
