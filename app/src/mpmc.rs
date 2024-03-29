//! Code adapted from embassy-util (https://github.com/embassy-rs/embassy/tree/master/embassy-util)
//!
//! A queue for sending values between asynchronous tasks.
//!
//! It can be used concurrently by multiple producers (senders) and multiple
//! consumers (receivers), i.e. it is an  "MPMC channel".
//!
//! Receivers are competing for messages. So a message that is received by
//! one receiver is not received by any other.
//!
//! This queue takes a Mutex type so that various
//! targets can be attained. For example, a ThreadModeMutex can be used
//! for single-core Cortex-M targets where messages are only passed
//! between tasks running in thread mode. Similarly, a CriticalSectionMutex
//! can also be used for single-core targets where messages are to be
//! passed from exception mode e.g. out of an interrupt handler.
//!
//! This module provides a bounded channel that has a limit on the number of
//! messages that it can store, and if this limit is reached, trying to send
//! another message will result in an error being returned.
//!

use core::cell::RefCell;
use core::fmt::Debug;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use defmt::{trace, debug};
use embassy_sync::blocking_mutex::{raw::RawMutex, Mutex};
use embassy_sync::waitqueue::WakerRegistration;
use futures::{Sink, Stream};

use heapless::Deque;

/// Send-only access to a [`Channel`].
#[derive(Copy)]
pub struct Sender<'ch, M, T, const N: usize>
where
    M: RawMutex,
{
    channel: &'ch Channel<M, T, N>,
}

impl<'ch, M, T, const N: usize> Clone for Sender<'ch, M, T, N>
where
    M: RawMutex,
{
    fn clone(&self) -> Self {
        Sender { channel: self.channel }
    }
}

impl<'ch, M, T, const N: usize> Sender<'ch, M, T, N>
where
    M: RawMutex,
{
    /// Sends a value.
    ///
    /// See [`Channel::send()`]
    pub fn send(&self, message: T) -> SendFuture<'ch, M, T, N> {
        trace!("foo");
        self.channel.send(message)
    }

    /// Attempt to immediately send a message.
    ///
    /// See [`Channel::send()`]
    pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
        self.channel.try_send(message)
    }
}

/// Send-only access to a [`Channel`] without knowing channel size.
#[derive(Copy)]
pub struct DynamicSender<'ch, T> {
    channel: &'ch dyn DynamicChannel<T>,
}

impl<'ch, T> Clone for DynamicSender<'ch, T> {
    fn clone(&self) -> Self {
        DynamicSender { channel: self.channel }
    }
}

impl<'ch, M, T, const N: usize> From<Sender<'ch, M, T, N>> for DynamicSender<'ch, T>
where
    M: RawMutex,
{
    fn from(s: Sender<'ch, M, T, N>) -> Self {
        Self { channel: s.channel }
    }
}

impl<'ch, T> DynamicSender<'ch, T> {
    /// Sends a value.
    ///
    /// See [`Channel::send()`]
    pub fn send(&self, message: T) -> DynamicSendFuture<'ch, T> {
        DynamicSendFuture {
            channel: self.channel,
            message: Some(message),
        }
    }

    /// Attempt to immediately send a message.
    ///
    /// See [`Channel::send()`]
    pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
        self.channel.try_send_with_context(message, None)
    }
}

/// Receive-only access to a [`Channel`].
#[derive(Copy)]
pub struct Receiver<'ch, M, T, const N: usize>
where
    M: RawMutex,
{
    channel: &'ch Channel<M, T, N>,
}

impl<'ch, M, T, const N: usize> Clone for Receiver<'ch, M, T, N>
where
    M: RawMutex,
{
    fn clone(&self) -> Self {
        Receiver { channel: self.channel }
    }
}

impl<'ch, M, T, const N: usize> Receiver<'ch, M, T, N>
where
    M: RawMutex,
{
    /// Receive the next value.
    ///
    /// See [`Channel::recv()`].
    pub fn recv(&self) -> RecvFuture<'_, M, T, N> {
        self.channel.recv()
    }

    /// Attempt to immediately receive the next value.
    ///
    /// See [`Channel::try_recv()`]
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.channel.try_recv()
    }
}

/// Receive-only access to a [`Channel`] without knowing channel size.
#[derive(Copy)]
pub struct DynamicReceiver<'ch, T> {
    channel: &'ch dyn DynamicChannel<T>,
}

impl<'ch, T> Clone for DynamicReceiver<'ch, T> {
    fn clone(&self) -> Self {
        DynamicReceiver { channel: self.channel }
    }
}

impl<'ch, T> DynamicReceiver<'ch, T> {
    /// Receive the next value.
    ///
    /// See [`Channel::recv()`].
    pub fn recv(&self) -> DynamicRecvFuture<'_, T> {
        DynamicRecvFuture { channel: self.channel }
    }

    /// Attempt to immediately receive the next value.
    ///
    /// See [`Channel::try_recv()`]
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.channel.try_recv_with_context(None)
    }
}

impl<'ch, M, T, const N: usize> From<Receiver<'ch, M, T, N>> for DynamicReceiver<'ch, T>
where
    M: RawMutex,
{
    fn from(s: Receiver<'ch, M, T, N>) -> Self {
        Self { channel: s.channel }
    }
}

/// Future returned by [`Channel::recv`] and  [`Receiver::recv`].
pub struct RecvFuture<'ch, M, T, const N: usize>
where
    M: RawMutex,
{
    channel: &'ch Channel<M, T, N>,
}

impl<'ch, M, T, const N: usize> Future for RecvFuture<'ch, M, T, N>
where
    M: RawMutex,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        match self.channel.try_recv_with_context(Some(cx)) {
            Ok(v) => Poll::Ready(v),
            Err(TryRecvError::Empty) => Poll::Pending,
        }
    }
}

/// Future returned by [`DynamicReceiver::recv`].
pub struct DynamicRecvFuture<'ch, T> {
    channel: &'ch dyn DynamicChannel<T>,
}

impl<'ch, T> Future for DynamicRecvFuture<'ch, T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        match self.channel.try_recv_with_context(Some(cx)) {
            Ok(v) => Poll::Ready(v),
            Err(TryRecvError::Empty) => Poll::Pending,
        }
    }
}

/// Future returned by [`Channel::send`] and  [`Sender::send`].
pub struct SendFuture<'ch, M, T, const N: usize>
where
    M: RawMutex,
{
    channel: &'ch Channel<M, T, N>,
    message: Option<T>,
}

impl<'ch, M, T, const N: usize> Future for SendFuture<'ch, M, T, N>
where
    M: RawMutex,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.message.take() {
            Some(m) => match self.channel.try_send_with_context(m, Some(cx)) {
                Ok(..) => Poll::Ready(()),
                Err(TrySendError::Full(m)) => {
                    self.message = Some(m);
                    Poll::Pending
                }
            },
            None => panic!("Message cannot be None"),
        }
    }
}

impl<'ch, M, T, const N: usize> Unpin for SendFuture<'ch, M, T, N> where M: RawMutex {}

/// Future returned by [`DynamicSender::send`].
pub struct DynamicSendFuture<'ch, T> {
    channel: &'ch dyn DynamicChannel<T>,
    message: Option<T>,
}

impl<'ch, T> Future for DynamicSendFuture<'ch, T> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.message.take() {
            Some(m) => match self.channel.try_send_with_context(m, Some(cx)) {
                Ok(..) => Poll::Ready(()),
                Err(TrySendError::Full(m)) => {
                    self.message = Some(m);
                    Poll::Pending
                }
            },
            None => panic!("Message cannot be None"),
        }
    }
}

impl<'ch, T> Unpin for DynamicSendFuture<'ch, T> {}

trait DynamicChannel<T> {
    fn try_send_with_context(&self, message: T, cx: Option<&mut Context<'_>>) -> Result<(), TrySendError<T>>;

    fn try_recv_with_context(&self, cx: Option<&mut Context<'_>>) -> Result<T, TryRecvError>;
}

/// Error returned by [`try_recv`](Channel::try_recv).
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TryRecvError {
    /// A message could not be received because the channel is empty.
    Empty,
}

/// Error returned by [`try_send`](Channel::try_send).
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TrySendError<T> {
    /// The data could not be sent on the channel because the channel is
    /// currently full and sending would require blocking.
    Full(T),
}

struct ChannelState<T, const N: usize> {
    queue: Deque<T, N>,
    receiver_waker: WakerRegistration,
    senders_waker: WakerRegistration,
}

impl<T, const N: usize> ChannelState<T, N> {
    const fn new() -> Self {
        ChannelState {
            queue: Deque::new(),
            receiver_waker: WakerRegistration::new(),
            senders_waker: WakerRegistration::new(),
        }
    }

    fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.try_recv_with_context(None)
    }

    fn try_recv_with_context(&mut self, cx: Option<&mut Context<'_>>) -> Result<T, TryRecvError> {
        if self.queue.is_full() {
            self.senders_waker.wake();
        }

        if let Some(message) = self.queue.pop_front() {
            Ok(message)
        } else {
            if let Some(cx) = cx {
                self.receiver_waker.register(cx.waker());
            }
            Err(TryRecvError::Empty)
        }
    }

    fn try_send(&mut self, message: T) -> Result<(), TrySendError<T>> {
        self.try_send_with_context(message, None)
    }

    fn try_send_with_context(&mut self, message: T, cx: Option<&mut Context<'_>>) -> Result<(), TrySendError<T>> {
        match self.queue.push_back(message) {
            Ok(()) => {
                self.receiver_waker.wake();
                Ok(())
            }
            Err(message) => {
                if let Some(cx) = cx {
                    self.senders_waker.register(cx.waker());
                }
                Err(TrySendError::Full(message))
            }
        }
    }
}

/// A bounded channel for communicating between asynchronous tasks
/// with backpressure.
///
/// The channel will buffer up to the provided number of messages.  Once the
/// buffer is full, attempts to `send` new messages will wait until a message is
/// received from the channel.
///
/// All data sent will become available in the same order as it was sent.
pub struct Channel<M, T, const N: usize>
where
    M: RawMutex,
{
    inner: Mutex<M, RefCell<ChannelState<T, N>>>,
}

impl<M, T, const N: usize> Channel<M, T, N>
where
    M: RawMutex,
{
    /// Establish a new bounded channel. For example, to create one with a NoopMutex:
    ///
    /// ```
    /// use embassy_util::channel::mpmc::Channel;
    /// use embassy_util::blocking_mutex::raw::NoopRawMutex;
    ///
    /// // Declare a bounded channel of 3 u32s.
    /// let mut channel = Channel::<NoopRawMutex, u32, 3>::new();
    /// ```
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(RefCell::new(ChannelState::new())),
        }
    }

    fn lock<R>(&self, f: impl FnOnce(&mut ChannelState<T, N>) -> R) -> R {
        self.inner.lock(|rc| f(&mut *rc.borrow_mut()))
    }

    fn try_recv_with_context(&self, cx: Option<&mut Context<'_>>) -> Result<T, TryRecvError> {
        self.lock(|c| c.try_recv_with_context(cx))
    }

    fn try_send_with_context(&self, m: T, cx: Option<&mut Context<'_>>) -> Result<(), TrySendError<T>> {
        self.lock(|c| c.try_send_with_context(m, cx))
    }

    /// Get a sender for this channel.
    pub fn sender(&self) -> Sender<'_, M, T, N> {
        Sender { channel: self }
    }

    /// Get a receiver for this channel.
    pub fn receiver(&self) -> Receiver<'_, M, T, N> {
        Receiver { channel: self }
    }

    /// Send a value, waiting until there is capacity.
    ///
    /// Sending completes when the value has been pushed to the channel's queue.
    /// This doesn't mean the value has been received yet.
    pub fn send(&self, message: T) -> SendFuture<'_, M, T, N> {
        SendFuture {
            channel: self,
            message: Some(message),
        }
    }

    /// Attempt to immediately send a message.
    ///
    /// This method differs from [`send`](Channel::send) by returning immediately if the channel's
    /// buffer is full, instead of waiting.
    ///
    /// # Errors
    ///
    /// If the channel capacity has been reached, i.e., the channel has `n`
    /// buffered values where `n` is the argument passed to [`Channel`], then an
    /// error is returned.
    pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
        self.lock(|c| c.try_send(message))
    }

    /// Receive the next value.
    ///
    /// If there are no messages in the channel's buffer, this method will
    /// wait until a message is sent.
    pub fn recv(&self) -> RecvFuture<'_, M, T, N> {
        RecvFuture { channel: self }
    }

    /// Attempt to immediately receive a message.
    ///
    /// This method will either receive a message from the channel immediately or return an error
    /// if the channel is empty.
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.lock(|c| c.try_recv())
    }
}

/// Implements the DynamicChannel to allow creating types that are unaware of the queue size with the
/// tradeoff cost of dynamic dispatch.
impl<M, T, const N: usize> DynamicChannel<T> for Channel<M, T, N>
where
    M: RawMutex,
{
    fn try_send_with_context(&self, m: T, cx: Option<&mut Context<'_>>) -> Result<(), TrySendError<T>> {
        Channel::try_send_with_context(self, m, cx)
    }

    fn try_recv_with_context(&self, cx: Option<&mut Context<'_>>) -> Result<T, TryRecvError> {
        Channel::try_recv_with_context(self, cx)
    }
}

impl<'t, M: RawMutex, T, const N: usize> Stream for Receiver<'t, M, T, N> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.channel.try_recv_with_context(Some(cx)) {
            Ok(v) => Poll::Ready(Some(v)),
            Err(TryRecvError::Empty) => Poll::Pending,
        }
    }
}

impl<'t, M: RawMutex, T, const N: usize> Sink<T> for Sender<'t, M, T, N> {
    type Error = TrySendError<T>;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.channel.lock(|state| {
            if state.queue.is_full() {
                state.receiver_waker.register(cx.waker());
                Poll::Pending
            } else {
                Poll::Ready(Ok(()))
            }
        })
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        self.channel.try_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
