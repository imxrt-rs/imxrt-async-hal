//! UART serial driver

use crate::{dma, instance::Inst, iomuxc, ral};
use core::fmt;

/// UART Serial driver
///
/// `UART` can send and receive byte buffers using a transfer / receive two-wire interface.
/// After constructing a `UART`, the baud rate is unspecified. Use [`set_baud`](#method.set_baud)
/// to configure your serial device.
///
/// The RAL instances are available in `ral::lpuart`.
///
/// # Example
///
/// Create a UART instance (LPUART2, 9600bps) using pins 14 and 15 that echos serial data.
///
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::{ccm::{self, ClockGate}, dma, iomuxc, UART, instance};
/// use hal::ral::{
///     ccm::CCM, lpuart::LPUART2,
///     dma0::DMA0, dmamux::DMAMUX,
///     iomuxc::IOMUXC,
/// };
///
/// let pads = IOMUXC::take().map(iomuxc::new).unwrap();
///
/// let mut ccm = CCM::take().map(ccm::CCM::from_ral).unwrap();
/// let mut dma = DMA0::take().unwrap();
/// ccm.handle.set_clock_gate_dma(&mut dma, ClockGate::On);
/// let mut channels = dma::channels(
///     dma,
///     DMAMUX::take().unwrap(),
/// );
/// let mut uart2 = LPUART2::take().and_then(instance::uart).unwrap();
///
/// let mut uart_clock = ccm.uart_clock.enable(&mut ccm.handle);
/// uart_clock.set_clock_gate(&mut uart2, ClockGate::On);
///
/// let mut uart = UART::new(
///     uart2,
///     pads.ad_b1.p02, // TX
///     pads.ad_b1.p03, // RX
///     channels[7].take().unwrap(),
///     &uart_clock,
/// );
///
/// uart.set_baud(9600).unwrap();
/// # async {
/// loop {
///     let mut buffer = [0; 1];
///     uart.read(&mut buffer).await.unwrap();
///     uart.write(&buffer).await.unwrap();
/// }
/// # };
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
pub struct UART<TX, RX> {
    uart: DmaCapable,
    channel: dma::Channel,
    tx: TX,
    rx: RX,
    hz: u32,
}

impl<TX, RX> fmt::Debug for UART<TX, RX> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UART{}", self.uart.inst())
    }
}

impl<TX, RX, M> UART<TX, RX>
where
    TX: iomuxc::uart::Pin<Direction = iomuxc::uart::TX, Module = M>,
    RX: iomuxc::uart::Pin<Direction = iomuxc::uart::RX, Module = M>,
    M: iomuxc::consts::Unsigned,
{
    /// Create a new `UART` from a UART instance, TX and RX pins, and a DMA channel
    ///
    /// The baud rate of the returned `UART` is unspecified. Make sure you use [`set_baud`](#method.set_baud)
    /// to properly configure the driver.
    pub fn new(
        uart: crate::instance::UART<M>,
        mut tx: TX,
        mut rx: RX,
        channel: dma::Channel,
        clock: &crate::ccm::UARTClock,
    ) -> UART<TX, RX> {
        crate::iomuxc::uart::prepare(&mut tx);
        crate::iomuxc::uart::prepare(&mut rx);

        let mut uart = UART {
            uart: DmaCapable {
                uart: uart.release(),
            },
            tx,
            rx,
            channel,
            hz: clock.frequency(),
        };
        let _ = uart.set_baud(9600);
        ral::modify_reg!(ral::lpuart, uart.uart, CTRL, TE: TE_1, RE: RE_1);
        uart
    }
}

impl<TX, RX> UART<TX, RX> {
    /// Set the serial baud rate
    ///
    /// If there is an error, the error is [`Error::Clock`](enum.UARTError.html#variant.Clock).
    pub fn set_baud(&mut self, baud: u32) -> Result<(), Error> {
        let timings = timings(self.hz, baud)?;
        self.while_disabled(|this| {
            ral::modify_reg!(
                ral::lpuart,
                this.uart,
                BAUD,
                OSR: u32::from(timings.osr),
                SBR: u32::from(timings.sbr),
                BOTHEDGE: u32::from(timings.both_edge)
            );
        });
        Ok(())
    }

    fn while_disabled<F: FnMut(&mut Self) -> R, R>(&mut self, mut act: F) -> R {
        ral::modify_reg!(
            ral::lpuart,
            self.uart,
            FIFO,
            TXFLUSH: TXFLUSH_1,
            RXFLUSH: RXFLUSH_1
        );
        let (te, re) = ral::read_reg!(ral::lpuart, self.uart, CTRL, TE, RE);
        ral::modify_reg!(ral::lpuart, self.uart, CTRL, TE: TE_0, RE: RE_0);
        let res = act(self);
        ral::modify_reg!(ral::lpuart, self.uart, CTRL, TE: te, RE: re);
        res
    }

    /// Return the pins, RAL instance, and DMA channel that comprise the UART driver
    pub fn release(self) -> (TX, RX, ral::lpuart::Instance, dma::Channel) {
        (self.tx, self.rx, self.uart.uart, self.channel)
    }

    /// Wait to receive a `buffer` of data
    ///
    /// Returns the number of bytes placed into `buffer`, or an error.
    pub async fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        let len = crate::dma::receive(&mut self.channel, &self.uart, buffer).await?;
        Ok(len)
    }

    /// Wait to send a `buffer` of data
    ///
    /// Returns the number of bytes sent from `buffer`, or an error.
    pub async fn write(&mut self, buffer: &[u8]) -> Result<usize, Error> {
        let len = crate::dma::transfer(&mut self.channel, &self.uart, buffer).await?;
        Ok(len)
    }
}

/// An opaque type that describes timing configurations
struct Timings {
    /// OSR register value. Accounts for the -1. May be written
    /// directly to the register
    osr: u8,
    /// True if we need to set BOTHEDGE given the OSR value
    both_edge: bool,
    /// SBR value;
    sbr: u16,
}

/// Errors propagated from a [`UART`](struct.UART.html) device
#[non_exhaustive]
#[derive(Debug)]
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
pub enum Error {
    /// There was an error when preparing the baud rate or clocks
    Clock,
    /// Error when preparing a DMA transaction
    DMA(dma::Error),
}

impl From<dma::Error> for Error {
    fn from(error: dma::Error) -> Self {
        Error::DMA(error)
    }
}

/// Compute timings for a UART peripheral. Returns the timings,
/// or a string describing an error.
fn timings(effective_clock: u32, baud: u32) -> Result<Timings, Error> {
    //        effective_clock
    // baud = ---------------
    //         (OSR+1)(SBR)
    //
    // Solve for SBR:
    //
    //       effective_clock
    // SBR = ---------------
    //        (OSR+1)(baud)
    //
    // After selecting SBR, calculate effective baud.
    // Minimize the error over all OSRs.

    let base_clock: u32 = effective_clock.checked_div(baud).ok_or(Error::Clock)?;
    let mut error = u32::max_value();
    let mut best_osr = 16;
    let mut best_sbr = 1;

    for osr in 4..=32 {
        let sbr = base_clock.checked_div(osr).ok_or(Error::Clock)?;
        let sbr = sbr.max(1).min(8191);
        let effective_baud = effective_clock.checked_div(osr * sbr).ok_or(Error::Clock)?;
        let err = effective_baud.max(baud) - effective_baud.min(baud);
        if err < error {
            best_osr = osr;
            best_sbr = sbr;
            error = err
        }
    }

    use core::convert::TryFrom;
    Ok(Timings {
        osr: u8::try_from(best_osr - 1).map_err(|_| Error::Clock)?,
        sbr: u16::try_from(best_sbr).map_err(|_| Error::Clock)?,
        both_edge: best_osr < 8,
    })
}

/// Adapter to support DMA peripheral traits
/// on RAL LPSPI instances
struct DmaCapable {
    uart: ral::lpuart::Instance,
}

impl core::ops::Deref for DmaCapable {
    type Target = ral::lpuart::Instance;
    fn deref(&self) -> &Self::Target {
        &self.uart
    }
}

unsafe impl dma::Destination<u8> for DmaCapable {
    fn destination_signal(&self) -> u32 {
        // Make sure that the match expression will never hit the unreachable!() case.
        // The comments and conditional compiles show what we're currently considering in
        // that match. If your chip isn't listed, it's not something we considered.
        #[cfg(not(any(feature = "imxrt1010", feature = "imxrt1060")))]
        compile_error!("Ensure that LPUART DMAMUX TX channels are correct");

        // See table 4-3 of the iMXRT1060 Reference Manual (Rev 2)
        match &*self.uart as *const _ {
            // imxrt1010, imxrt1060
            ral::lpuart::LPUART1 => 2,
            // imxrt1010, imxrt1060
            ral::lpuart::LPUART2 => 66,
            // imxrt1010, imxrt1060
            ral::lpuart::LPUART3 => 4,
            // imxrt1010, imxrt1060
            ral::lpuart::LPUART4 => 68,
            #[cfg(feature = "imxrt1060")]
            ral::lpuart::LPUART5 => 6,
            #[cfg(feature = "imxrt1060")]
            ral::lpuart::LPUART6 => 70,
            #[cfg(feature = "imxrt1060")]
            ral::lpuart::LPUART7 => 8,
            #[cfg(feature = "imxrt1060")]
            ral::lpuart::LPUART8 => 72,
            _ => unreachable!(),
        }
    }
    fn destination(&self) -> *const u8 {
        &self.uart.DATA as *const _ as *const u8
    }
    fn enable_destination(&self) {
        ral::modify_reg!(ral::lpuart, self.uart, BAUD, TDMAE: 1);
    }
    fn disable_destination(&self) {
        while ral::read_reg!(ral::lpuart, self.uart, BAUD, TDMAE == 1) {
            ral::modify_reg!(ral::lpuart, self.uart, BAUD, TDMAE: 0);
        }
    }
}

unsafe impl dma::Source<u8> for DmaCapable {
    fn source_signal(&self) -> u32 {
        // Make sure that the match expression will never hit the unreachable!() case.
        // The comments and conditional compiles show what we're currently considering in
        // that match. If your chip isn't listed, it's not something we considered.
        #[cfg(not(any(feature = "imxrt1010", feature = "imxrt1060")))]
        compile_error!("Ensure that LPUART DMAMUX RX channels are correct");

        // See table 4-3 of the iMXRT1060 Reference Manual (Rev 2)
        match &*self.uart as *const _ {
            // imxrt1010, imxrt1060
            ral::lpuart::LPUART1 => 3,
            // imxrt1010, imxrt1060
            ral::lpuart::LPUART2 => 67,
            // imxrt1010, imxrt1060
            ral::lpuart::LPUART3 => 5,
            // imxrt1010, imxrt1060
            ral::lpuart::LPUART4 => 69,
            #[cfg(feature = "imxrt1060")]
            ral::lpuart::LPUART5 => 7,
            #[cfg(feature = "imxrt1060")]
            ral::lpuart::LPUART6 => 71,
            #[cfg(feature = "imxrt1060")]
            ral::lpuart::LPUART7 => 9,
            #[cfg(feature = "imxrt1060")]
            ral::lpuart::LPUART8 => 73,
            _ => unreachable!(),
        }
    }
    fn source(&self) -> *const u8 {
        &self.uart.DATA as *const _ as *const u8
    }
    fn enable_source(&self) {
        // Clear all status flags
        ral::modify_reg!(
            ral::lpuart,
            self.uart,
            STAT,
            IDLE: IDLE_1,
            OR: OR_1,
            NF: NF_1,
            FE: FE_1,
            PF: PF_1
        );
        ral::modify_reg!(ral::lpuart, self.uart, BAUD, RDMAE: 1);
    }
    fn disable_source(&self) {
        while ral::read_reg!(ral::lpuart, self.uart, BAUD, RDMAE == 1) {
            ral::modify_reg!(ral::lpuart, self.uart, BAUD, RDMAE: 0);
        }
    }
}

/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::ral::{ccm::CCM, lpuart::LPUART2};
///
/// let hal::ccm::CCM {
///     mut handle,
///     uart_clock,
///     ..
/// } = CCM::take().map(hal::ccm::CCM::from_ral).unwrap();
/// let mut uart_clock = uart_clock.enable(&mut handle);
/// let mut uart2 = LPUART2::take().unwrap();
/// uart_clock.set_clock_gate(&mut uart2, hal::ccm::ClockGate::On);
/// ```
#[cfg(doctest)]
struct ClockingWeakRalInstance;

/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::ral::{ccm::CCM, lpuart::LPUART2};
///
/// let hal::ccm::CCM {
///     mut handle,
///     uart_clock,
///     ..
/// } = CCM::take().map(hal::ccm::CCM::from_ral).unwrap();
/// let mut uart_clock = uart_clock.enable(&mut handle);
/// let mut uart2: hal::instance::UART<hal::iomuxc::consts::U2> = LPUART2::take()
///     .and_then(hal::instance::uart)
///     .unwrap();
/// uart_clock.set_clock_gate(&mut uart2, hal::ccm::ClockGate::On);
/// ```
#[cfg(doctest)]
struct ClockingStrongHalInstance;
