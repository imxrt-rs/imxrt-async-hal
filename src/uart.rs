//! UART serial driver

use crate::{dma, iomuxc, ral};
use core::fmt;

/// UART Serial driver
///
/// `UART` can send and receive byte buffers using a transfer / receive two-wire interface.
/// After constructing a `UART`, the baud rate is unspecified. Use [`set_baud`](UART::set_baud())
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
/// use hal::{dma, iomuxc, UART};
/// use hal::ral::{self,
///     ccm::CCM, lpuart::LPUART2,
///     dma0::DMA0, dmamux::DMAMUX,
///     iomuxc::IOMUXC,
/// };
///
///
/// const SOURCE_CLOCK_HZ: u32 = 24_000_000; // XTAL
/// const SOURCE_CLOCK_DIVIDER: u32 = 1;
///
/// let pads = IOMUXC::take().map(iomuxc::new).unwrap();
///
/// let ccm = CCM::take().unwrap();
/// // Select 24MHz XTAL as clock source, no divider...
/// ral::modify_reg!(ral::ccm, ccm, CSCDR1, UART_CLK_SEL: 1 /* Oscillator */, UART_CLK_PODF: SOURCE_CLOCK_DIVIDER - 1);
/// // Enable LPUART2 clock gate...
/// ral::modify_reg!(ral::ccm, ccm, CCGR0, CG14: 0b11);
/// // DMA clock gate on
/// ral::modify_reg!(ral::ccm, ccm, CCGR5, CG3: 0b11);
///
/// let dma = DMA0::take().unwrap();
/// let mut channels = dma::channels(
///     dma,
///     DMAMUX::take().unwrap(),
/// );
/// let uart2 = LPUART2::take().unwrap();
///
/// let mut uart = UART::new(
///     uart2,
///     pads.ad_b1.p02, // TX
///     pads.ad_b1.p03, // RX
/// );
/// let mut channel = channels[7].take().unwrap();
/// channel.set_interrupt_on_completion(true);
///
/// uart.set_baud(9600, SOURCE_CLOCK_HZ / SOURCE_CLOCK_DIVIDER).unwrap();
/// # async {
/// loop {
///     let mut buffer = [0; 1];
///     uart.dma_read(&mut channel, &mut buffer).await.unwrap();
///     uart.dma_write(&mut channel, &buffer).await.unwrap();
/// }
/// # };
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
pub struct UART<N, TX, RX> {
    uart: ral::lpuart::Instance<N>,
    tx: TX,
    rx: RX,
}

impl<N: iomuxc::consts::Unsigned, TX, RX> fmt::Debug for UART<N, TX, RX> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UART{}", N::USIZE)
    }
}

impl<M, TX, RX> UART<M, TX, RX>
where
    TX: iomuxc::uart::Pin<Direction = iomuxc::uart::TX, Module = M>,
    RX: iomuxc::uart::Pin<Direction = iomuxc::uart::RX, Module = M>,
    M: iomuxc::consts::Unsigned,
{
    /// Create a new `UART` from a UART instance, and TX and RX pins
    ///
    /// The baud rate of the returned `UART` is unspecified. Make sure you use [`set_baud`](UART::set_baud())
    /// to properly configure the driver.
    pub fn new(uart: crate::ral::lpuart::Instance<M>, mut tx: TX, mut rx: RX) -> Self {
        crate::iomuxc::uart::prepare(&mut tx);
        crate::iomuxc::uart::prepare(&mut rx);

        let uart = UART { uart, tx, rx };
        ral::modify_reg!(ral::lpuart, uart.uart, CTRL, TE: TE_1, RE: RE_1);
        uart
    }
}

impl<N, TX, RX> UART<N, TX, RX> {
    /// Set the serial baud rate
    ///
    /// If there is an error, the error is [`Error::Clock`](Error::Clock).
    pub fn set_baud(&mut self, baud: u32, source_clock_hz: u32) -> Result<(), Error> {
        let timings = timings(source_clock_hz, baud)?;
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

    /// Return the pins and RAL instance that comprise the UART driver
    pub fn release(self) -> (TX, RX, ral::lpuart::Instance<N>) {
        (self.tx, self.rx, self.uart)
    }

    /// Use a DMA channel to write data to the UART peripheral
    ///
    /// Completes when all data in `buffer` has been written to the UART
    /// peripheral.
    pub fn dma_write<'a>(
        &'a mut self,
        channel: &'a mut dma::Channel,
        buffer: &'a [u8],
    ) -> dma::Tx<'a, Self, u8> {
        dma::transfer(channel, buffer, self)
    }

    /// Use a DMA channel to read data from the UART peripheral
    ///
    /// Completes when `buffer` is filled.
    pub fn dma_read<'a>(
        &'a mut self,
        channel: &'a mut dma::Channel,
        buffer: &'a mut [u8],
    ) -> dma::Rx<'a, Self, u8> {
        dma::receive(channel, self, buffer)
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

/// Errors propagated from a [`UART`] device
#[non_exhaustive]
#[derive(Debug)]
#[cfg_attr(docsrs, doc(cfg(feature = "uart")))]
pub enum Error {
    /// There was an error when preparing the baud rate or clocks
    Clock,
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

unsafe impl<N, TX, RX> dma::Destination<u8> for UART<N, TX, RX> {
    fn destination_signal(&self) -> u32 {
        use dma::Source;
        self.source_signal() - 1
    }
    fn destination_address(&self) -> *const u8 {
        &self.uart.DATA as *const _ as *const u8
    }
    fn enable_destination(&mut self) {
        ral::modify_reg!(ral::lpuart, self.uart, BAUD, TDMAE: 1);
    }
    fn disable_destination(&mut self) {
        while ral::read_reg!(ral::lpuart, self.uart, BAUD, TDMAE == 1) {
            ral::modify_reg!(ral::lpuart, self.uart, BAUD, TDMAE: 0);
        }
    }
}

unsafe impl<N, TX, RX> dma::Source<u8> for UART<N, TX, RX> {
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
    fn source_address(&self) -> *const u8 {
        &self.uart.DATA as *const _ as *const u8
    }
    fn enable_source(&mut self) {
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
    fn disable_source(&mut self) {
        while ral::read_reg!(ral::lpuart, self.uart, BAUD, RDMAE == 1) {
            ral::modify_reg!(ral::lpuart, self.uart, BAUD, RDMAE: 0);
        }
    }
}
