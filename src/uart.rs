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
/// use hal::{ccm, dma, iomuxc, UART, instance};
/// use hal::ral::{
///     ccm::CCM, lpuart::LPUART2,
///     dma0::DMA0, dmamux::DMAMUX,
///     iomuxc::IOMUXC,
/// };
///
/// let pads = IOMUXC::take().map(iomuxc::new).unwrap();
///
/// let mut ccm = CCM::take().map(ccm::CCM::new).unwrap();
/// let mut channels = dma::channels(
///     DMA0::take().unwrap(),
///     DMAMUX::take().unwrap(),
///     &mut ccm.handle
/// );
/// let uart2 = LPUART2::take().and_then(instance::uart).unwrap();
/// let mut uart = UART::new(
///     uart2,
///     pads.ad_b1.p02, // TX
///     pads.ad_b1.p03, // RX
///     channels[7].take().unwrap(),
///     &mut ccm.handle
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
pub struct UART<TX, RX> {
    uart: ral::lpuart::Instance,
    channel: dma::Channel,
    tx: TX,
    rx: RX,
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
        ccm: &mut crate::ccm::Handle,
    ) -> UART<TX, RX> {
        enable_clocks(ccm);

        crate::iomuxc::uart::prepare(&mut tx);
        crate::iomuxc::uart::prepare(&mut rx);

        let mut uart = UART {
            uart: uart.release(),
            tx,
            rx,
            channel,
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
        let timings = timings(UART_CLOCK, baud)?;
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
        (self.tx, self.rx, self.uart, self.channel)
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

const UART_CLOCK: u32 = crate::ccm::OSCILLATOR_FREQUENCY_HZ;

fn enable_clocks(ccm: &mut crate::ccm::Handle) {
    static ONCE: crate::once::Once = crate::once::new();
    ONCE.call(|| {
        // -----------------------------------------
        // Disable clocks before modifying selection
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CCGR5,
            CG12: 0,    // UART1
            CG13: 0     // UART7
        );
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CCGR0,
            CG14: 0,    // UART2
            CG6: 0      // UART3
        );
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CCGR1,
            CG12: 0     // UART4
        );
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CCGR3,
            CG1: 0,     // UART5
            CG3: 0      // UART6
        );
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CCGR6,
            CG7: 0      // UART8
        );
        // -----------------------------------------

        // -------------------------
        // Select clocks & prescalar
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CSCDR1,
            UART_CLK_SEL: UART_CLK_SEL_1, // Oscillator
            UART_CLK_PODF: DIVIDE_1
        );
        // -------------------------

        // -------------
        // Enable clocks
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CCGR5,
            CG12: 0b11,    // UART1
            CG13: 0b11     // UART7
        );
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CCGR0,
            CG14: 0b11,    // UART2
            CG6: 0b11      // UART3
        );
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CCGR1,
            CG12: 0b11     // UART4
        );
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CCGR3,
            CG1: 0b11,     // UART5
            CG3: 0b11      // UART6
        );
        ral::modify_reg!(
            ral::ccm,
            ccm.0,
            CCGR6,
            CG7: 0b11      // UART8
        );
    });
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

impl dma::Destination<u8> for ral::lpuart::Instance {
    fn destination_signal(&self) -> u32 {
        // See table 4-3 of the iMXRT1060 Reference Manual (Rev 2)
        match &**self as *const _ {
            ral::lpuart::LPUART1 => 2,
            ral::lpuart::LPUART2 => 66,
            ral::lpuart::LPUART3 => 4,
            ral::lpuart::LPUART4 => 68,
            ral::lpuart::LPUART5 => 6,
            ral::lpuart::LPUART6 => 70,
            ral::lpuart::LPUART7 => 8,
            ral::lpuart::LPUART8 => 72,
            _ => unreachable!(),
        }
    }
    fn destination(&self) -> *const u8 {
        &self.DATA as *const _ as *const u8
    }
    fn enable_destination(&self) {
        ral::modify_reg!(ral::lpuart, self, BAUD, TDMAE: 1);
    }
    fn disable_destination(&self) {
        while ral::read_reg!(ral::lpuart, self, BAUD, TDMAE == 1) {
            ral::modify_reg!(ral::lpuart, self, BAUD, TDMAE: 0);
        }
    }
}

impl dma::Source<u8> for ral::lpuart::Instance {
    fn source_signal(&self) -> u32 {
        // See table 4-3 of the iMXRT1060 Reference Manual (Rev 2)
        match &**self as *const _ {
            ral::lpuart::LPUART1 => 3,
            ral::lpuart::LPUART2 => 67,
            ral::lpuart::LPUART3 => 5,
            ral::lpuart::LPUART4 => 69,
            ral::lpuart::LPUART5 => 7,
            ral::lpuart::LPUART6 => 71,
            ral::lpuart::LPUART7 => 9,
            ral::lpuart::LPUART8 => 73,
            _ => unreachable!(),
        }
    }
    fn source(&self) -> *const u8 {
        &self.DATA as *const _ as *const u8
    }
    fn enable_source(&self) {
        // Clear all status flags
        ral::modify_reg!(
            ral::lpuart,
            self,
            STAT,
            IDLE: IDLE_1,
            OR: OR_1,
            NF: NF_1,
            FE: FE_1,
            PF: PF_1
        );
        ral::modify_reg!(ral::lpuart, self, BAUD, RDMAE: 1);
    }
    fn disable_source(&self) {
        while ral::read_reg!(ral::lpuart, self, BAUD, RDMAE == 1) {
            ral::modify_reg!(ral::lpuart, self, BAUD, RDMAE: 0);
        }
    }
}
