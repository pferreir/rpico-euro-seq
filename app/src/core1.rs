use core::fmt::{Debug, Display};

use alloc::{borrow::ToOwned, format, string::String};
use embassy_executor::time::{Timer, Duration};
use embassy_util::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_util::channel::signal::Signal;
use embedded_hal::blocking::spi::{Transfer, Write};
use embedded_sdmmc::sdmmc::{Error as ESCMMCSPIError, SdMmcSpi};
use futures::StreamExt;
use futures::{future::join};
use logic::stdlib::{
    FileSystem, StdlibError, Task, TaskId, TaskInterface, TaskManager, TaskReturn, TaskType,
};
use rp2040_hal::gpio::{
    bank0::{Gpio10, Gpio11, Gpio12, Gpio4, Gpio5, Gpio8, Gpio9},
    FunctionSpi, Pin, PushPullOutput,
};
use shared_bus::BusManagerSimple;

use crate::debounce::DebounceCallback;
use crate::{debounce, mpmc::{self, TryRecvError}};
use crate::{
    gate_cv::{self, GateCVOutWithPins},
    DummyTime,
};


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

pub struct EmbeddedTaskInterface<'t> {
    consumer: mpmc::Receiver<'t, CriticalSectionRawMutex, TaskReturn, 16>,
    producer: mpmc::Sender<'t, CriticalSectionRawMutex, Task, 16>,
    id_count: u32,
}

#[derive(Debug)]
pub enum TaskInterfaceError {
    QueueFull,
    TryRecvError(TryRecvError),
}

impl<'t> EmbeddedTaskInterface<'t> {
    pub fn new(
        consumer: mpmc::Receiver<'t, CriticalSectionRawMutex, TaskReturn, 16>,
        producer: mpmc::Sender<'t, CriticalSectionRawMutex, Task, 16>,
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
        let res = self
            .producer
            .try_send(Task(id, task_type))
            .map_err(|_| TaskInterfaceError::QueueFull)
            .map(|_| id);
        res
    }

    fn pop(&mut self) -> Result<Option<TaskReturn>, Self::Error> {
        Ok(Some(
            self.consumer
                .try_recv()
                .map_err(TaskInterfaceError::TryRecvError)?,
        ))
    }
}

async fn update_output<SPI: Transfer<u8> + Write<u8>>(mut output: GateCVOutWithPins<SPI>)
where
    <SPI as Transfer<u8>>::Error: Debug,
    <SPI as Write<u8>>::Error: Debug,
{
    loop {
        output.update();
        Timer::after(Duration::from_millis(1000)).await;
    }
}

async fn debouncing_task<'t>(
    mut rx: mpmc::Receiver<'t, CriticalSectionRawMutex, (u8, u8, DebounceCallback), 16>,
) {
    while let Some((num_slice, num_pin, callback)) = rx.next().await {
        debounce::debounce(num_slice, num_pin, callback).await;
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

pub async fn core1_task<'t, SPI: Transfer<u8> + Write<u8> + 't>(
    ready_signal: &Signal<bool>,
    mut rx_tasks: mpmc::Receiver<'t, CriticalSectionRawMutex, Task, 16>,
    mut tx_task_results: mpmc::Sender<'t, CriticalSectionRawMutex, TaskReturn, 16>,
    rx_debounces: mpmc::Receiver<'t, CriticalSectionRawMutex, (u8, u8, DebounceCallback), 16>,
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

    ready_signal.signal(true);

    join(
        task_manager.run_tasks(&mut rx_tasks, &mut tx_task_results),
        join(
            debouncing_task(rx_debounces),
            update_output(output),
        )
    )
    .await;

    Ok(())
}
