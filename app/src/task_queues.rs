use core::fmt::{Debug, Display};
use core::task::Poll;

use alloc::{borrow::ToOwned, format, string::String};
use critical_section::with;
use defmt::trace;
use embassy_util::waitqueue::AtomicWaker;
use embedded_hal::blocking::spi::{Transfer, Write};
use embedded_sdmmc::sdmmc::{Error as ESCMMCSPIError, SdMmcSpi};
use futures::{future::join, Sink, Stream};
use heapless::spsc;
use logic::stdlib::{
    FileSystem, StdlibError, Task, TaskId, TaskInterface, TaskManager, TaskReturn, TaskType,
};
use rp2040_hal::gpio::{
    bank0::{Gpio10, Gpio11, Gpio12, Gpio4, Gpio5, Gpio8, Gpio9},
    FunctionSpi, Pin, PushPullOutput,
};
use shared_bus::BusManagerSimple;

use crate::alarms::fire_alarm;
use crate::{
    gate_cv::{self, GateCVOutWithPins},
    DummyTime,
};

static TASK_WAKER: AtomicWaker = AtomicWaker::new();

fn format_spi_error<'t>(e: &ESCMMCSPIError) -> String {
    match e {
        ESCMMCSPIError::Transport => "Transport error from SPI Peripheral".to_owned(),
        ESCMMCSPIError::CantEnableCRC => "Failed to enable CRC checking".to_owned(),
        ESCMMCSPIError::TimeoutReadBuffer => "Timeout reading to buffer".to_owned(),
        ESCMMCSPIError::TimeoutWaitNotBusy => "Timeout waiting for card".to_owned(),
        ESCMMCSPIError::TimeoutCommand(cmd) => format!("Timeout executing command {}", cmd),
        ESCMMCSPIError::TimeoutACommand(cmd) => {
            format!("Timeout executing application-specific command {}", cmd)
        }
        ESCMMCSPIError::Cmd58Error => "Bad response from command 58".to_owned(),
        ESCMMCSPIError::RegisterReadError => "Error reading Card Specific Data Register".to_owned(),
        ESCMMCSPIError::CrcError(recv, exp) => {
            format!("CRC mismatch (got {}, expected {})", recv, exp)
        }
        ESCMMCSPIError::ReadError => "Error reading from card".to_owned(),
        ESCMMCSPIError::WriteError => "Error writing to card".to_owned(),
        ESCMMCSPIError::BadState => {
            "Can't perform this operation with the card in this state".to_owned()
        }
        ESCMMCSPIError::CardNotFound => "Card not found".to_owned(),
        ESCMMCSPIError::GpioError => "Couldn't set a GPIO pin".to_owned(),
    }
}

pub struct TaskChannelConsumer(pub spsc::Consumer<'static, Task, 128>);
pub struct TaskChannelProducer(pub spsc::Producer<'static, TaskReturn, 128>);

impl Stream for TaskChannelConsumer {
    type Item = Task;

    fn poll_next(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Option<Self::Item>> {
        match self.0.dequeue() {
            Some(t) => Poll::Ready(Some(t)),
            None => {
                TASK_WAKER.register(cx.waker());
                Poll::Pending
            },
        }
    }
}

pub enum TaskChannelError {
    QueueFull,
}

impl Sink<TaskReturn> for TaskChannelProducer {
    type Error = TaskChannelError;

    fn poll_ready(
        self: core::pin::Pin<&mut Self>,
        _cx: &mut core::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        if self.0.ready() {
            Poll::Ready(Ok(()))
        } else {
            // TODO: handle this?
            Poll::Pending
        }
    }

    fn start_send(
        mut self: core::pin::Pin<&mut Self>,
        item: TaskReturn,
    ) -> Result<(), Self::Error> {
        self.0
            .enqueue(item)
            .map_err(|_| TaskChannelError::QueueFull)
    }

    fn poll_flush(
        self: core::pin::Pin<&mut Self>,
        _cx: &mut core::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: core::pin::Pin<&mut Self>,
        _cx: &mut core::task::Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

pub struct EmbeddedTaskInterface<'t> {
    consumer: spsc::Consumer<'t, TaskReturn, 128>,
    producer: spsc::Producer<'t, Task, 128>,
    id_count: u32,
}

#[derive(Debug)]
pub enum TaskInterfaceError {
    QueueFull,
}

impl<'t> EmbeddedTaskInterface<'t> {
    pub fn new(
        consumer: spsc::Consumer<'t, TaskReturn, 128>,
        producer: spsc::Producer<'t, Task, 128>,
    ) -> Self {
        Self {
            consumer,
            producer,
            id_count: 0,
        }
    }
}

impl<'t> TaskInterface for EmbeddedTaskInterface<'t> {
    type Error = TaskInterfaceError;

    fn submit(&mut self, task_type: TaskType) -> Result<TaskId, Self::Error> {
        let id = self.id_count;
        self.id_count += 1;
        let res = self.producer
            .enqueue(Task(id, task_type))
            .map_err(|_| TaskInterfaceError::QueueFull)
            .map(|_| id);
        TASK_WAKER.wake();
        res
    }

    fn pop(&mut self) -> Result<Option<TaskReturn>, Self::Error> {
        Ok(self.consumer.dequeue())
    }
}

async fn update_output<SPI: Transfer<u8> + Write<u8>>(mut output: GateCVOutWithPins<SPI>)
where
    <SPI as Transfer<u8>>::Error: Debug,
    <SPI as Write<u8>>::Error: Debug,
{
    loop {
        output.update();
    }
}

#[derive(Debug)]
pub enum TaskManagerTaskError {
    Stdlib(StdlibError),
    SPI(ESCMMCSPIError),
}

impl Display for TaskManagerTaskError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TaskManagerTaskError::Stdlib(err) => err.fmt(f),
            TaskManagerTaskError::SPI(err) => f.write_str(&format_spi_error(err)),
        }
    }
}

pub async fn task_manager<'t, SPI: Transfer<u8> + Write<u8> + 't>(
    mut rx: TaskChannelConsumer,
    mut tx: TaskChannelProducer,
    spi_bus: BusManagerSimple<SPI>,
    (clk, _miso, mosi, cs_out, cs_mmc, gate1, gate2): (
        Pin<Gpio10, FunctionSpi>,
        Pin<Gpio12, FunctionSpi>,
        Pin<Gpio11, FunctionSpi>,
        Pin<Gpio9, PushPullOutput>,
        Pin<Gpio8, PushPullOutput>,
        Pin<Gpio4, PushPullOutput>,
        Pin<Gpio5, PushPullOutput>,
    ),
) -> Result<(), TaskManagerTaskError>
where
    <SPI as Transfer<u8>>::Error: Debug,
    <SPI as Write<u8>>::Error: Debug,
{
    let spi = SdMmcSpi::new(spi_bus.acquire_spi(), cs_mmc);
    let bspi = spi.acquire().await.map_err(TaskManagerTaskError::SPI)?;

    let fs = FileSystem::new(bspi, DummyTime)
        .await
        .map_err(TaskManagerTaskError::Stdlib)?;
    let mut task_manager = TaskManager::new(fs);

    let output = gate_cv::GateCVOut::new(
        // DAC
        spi_bus.acquire_spi(),
        clk,
        mosi,
        cs_out,
        // gates
        gate1,
        gate2,
    );

    //join(
        task_manager.run_tasks(&mut rx, &mut tx)
        //update_output(output),
    //)
    .await;

    Ok(())
}
