use crate::{dma, instance, iomuxc, ral};

/// Pins for a SPI device
///
/// Consider using type aliases to simplify your [`SPI`] usage:
///
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::iomuxc::pads::b0::*;
///
/// // SPI pins used in my application
/// type SPIPins = hal::SPIPins<
///     B0_02,
///     B0_01,
///     B0_03,
///     B0_00,
/// >;
///
/// // Helper type for your SPI peripheral
/// type SPI = hal::SPI<SPIPins>;
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
pub struct Pins<SDO, SDI, SCK, PCS0> {
    /// Serial data out
    ///
    /// Data travels from the SPI host controller to the SPI device.
    pub sdo: SDO,
    /// Serial data in
    ///
    /// Data travels from the SPI device to the SPI host controller.
    pub sdi: SDI,
    /// Serial clock
    pub sck: SCK,
    /// Chip select 0
    ///
    /// (PCSx) convention matches the hardware.
    pub pcs0: PCS0,
}

/// Serial Peripheral Interface (SPI)
///
/// A `SPI` peripheral uses DMA for asynchronous I/O. Using up to two DMA channels, `SPI` peripherals
/// can perform SPI device reads, writes, and full-duplex transfers with `u8` and `u16` elements.
///
/// The SPI serial clock speed after construction is unspecified. Use [`set_clock_speed`](SPI::set_clock_speed())
/// to choose your SPI serial clock speed.
///
/// The RAL instances are available in `ral::lpspi`.
///
/// # Example
///
/// Perform a full-duplex SPI transfer of four `u16`s using SPI4.
///
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::{ccm::{self, ClockGate}, dma, instance, iomuxc, SPI, SPIPins};
/// use hal::ral::{
///     ccm::CCM, dma0::DMA0, dmamux::DMAMUX,
///     iomuxc::IOMUXC, lpspi::LPSPI4,
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
///
/// let mut spi_clock = ccm.spi_clock.enable(&mut ccm.handle);
/// let spi_pins = SPIPins {
///     sdo: pads.b0.p02,
///     sdi: pads.b0.p01,
///     sck: pads.b0.p03,
///     pcs0: pads.b0.p00,
/// };
/// let mut spi4 = LPSPI4::take().and_then(instance::spi).unwrap();
/// spi_clock.set_clock_gate(&mut spi4, ClockGate::On);
/// let mut spi = SPI::new(
///     spi_pins,
///     spi4,
///     (channels[8].take().unwrap(), channels[9].take().unwrap()),
///     &spi_clock,
/// );
///
/// spi.set_clock_speed(1_000_000).unwrap();
///
/// # async {
/// let mut buffer = [1, 2, 3, 4];
/// // Transmit the u16 words in buffer, and receive the reply into buffer.
/// spi.full_duplex_u16(&mut buffer).await;
/// # };
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
pub struct SPI<Pins> {
    pins: Pins,
    spi: DmaCapable,
    tx_channel: dma::Channel,
    rx_channel: dma::Channel,
    hz: u32,
}

const DEFAULT_CLOCK_SPEED_HZ: u32 = 8_000_000;

impl<SDO, SDI, SCK, PCS0, M> SPI<Pins<SDO, SDI, SCK, PCS0>>
where
    SDO: iomuxc::spi::Pin<Module = M, Signal = iomuxc::spi::SDO>,
    SDI: iomuxc::spi::Pin<Module = M, Signal = iomuxc::spi::SDI>,
    SCK: iomuxc::spi::Pin<Module = M, Signal = iomuxc::spi::SCK>,
    PCS0: iomuxc::spi::Pin<Module = M, Signal = iomuxc::spi::PCS0>,
    M: iomuxc::consts::Unsigned,
{
    /// Create a `SPI` from a set of pins, a SPI peripheral instance, and two DMA channels
    ///
    /// See the [`instance` module](instance) for more information on SPI peripheral
    /// instances.
    pub fn new(
        mut pins: Pins<SDO, SDI, SCK, PCS0>,
        spi: instance::SPI<M>,
        channels: (dma::Channel, dma::Channel),
        clock: &crate::ccm::SPIClock,
    ) -> Self {
        iomuxc::spi::prepare(&mut pins.sdo);
        iomuxc::spi::prepare(&mut pins.sdi);
        iomuxc::spi::prepare(&mut pins.sck);
        iomuxc::spi::prepare(&mut pins.pcs0);

        let spi = spi.release();

        ral::write_reg!(ral::lpspi, spi, CR, RST: RST_1);
        ral::write_reg!(ral::lpspi, spi, CR, RST: RST_0);
        set_clock_speed(&spi, clock.frequency(), DEFAULT_CLOCK_SPEED_HZ);
        ral::write_reg!(ral::lpspi, spi, CFGR1, MASTER: MASTER_1, SAMPLE: SAMPLE_1);
        // spi.set_mode(embedded_hal::spi::MODE_0).unwrap();
        ral::write_reg!(ral::lpspi, spi, FCR, RXWATER: 0xF, TXWATER: 0xF);
        ral::write_reg!(ral::lpspi, spi, CR, MEN: MEN_1);

        SPI {
            pins,
            spi: DmaCapable { spi },
            tx_channel: channels.0,
            rx_channel: channels.1,
            hz: clock.frequency(),
        }
    }

    /// Return the pins, instance, and DMA channels that are used in this `SPI`
    /// driver
    pub fn release(
        self,
    ) -> (
        Pins<SDO, SDI, SCK, PCS0>,
        ral::lpspi::Instance,
        (dma::Channel, dma::Channel),
    ) {
        (self.pins, self.spi.spi, (self.tx_channel, self.rx_channel))
    }
}

/// Errors propagated from a [`SPI`] device
#[non_exhaustive]
#[derive(Debug)]
#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
pub enum Error {
    /// Error when coordinating a DMA transaction
    DMA(dma::Error),
    /// Error when configuring the SPI serial clock
    ClockSpeed,
}

impl From<dma::Error> for Error {
    fn from(err: dma::Error) -> Self {
        Error::DMA(err)
    }
}

impl<Pins> SPI<Pins> {
    fn with_master_disabled<F: FnMut() -> R, R>(&self, mut act: F) -> R {
        let men = ral::read_reg!(ral::lpspi, self.spi, CR, MEN == MEN_1);
        ral::modify_reg!(ral::lpspi, self.spi, CR, MEN: MEN_0);
        let res = act();
        ral::modify_reg!(ral::lpspi, self.spi, CR, MEN: (men as u32));
        res
    }

    /// Set the SPI master clock speed
    ///
    /// Consider calling `set_clock_speed` after creating a `SPI`, since the clock speed after
    /// construction is unspecified.
    ///
    /// If an error occurs, it's an [`crate::spi::Error::ClockSpeed`].
    pub fn set_clock_speed(&mut self, hz: u32) -> Result<(), Error> {
        self.with_master_disabled(|| {
            // Safety: master is disabled
            set_clock_speed(&self.spi, self.hz, hz);
            Ok(())
        })
    }

    /// Await for a `u8` `buffer` of data from a SPI device
    ///
    /// Blocks until `buffer` is filled. Returns the number of bytes
    /// placed in `buffer`.
    pub async fn read_u8(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        let len = dma::receive(&mut self.rx_channel, &self.spi, buffer).await?;
        Ok(len)
    }

    /// Await for a `u16` `buffer` of data from a SPI device
    ///
    /// Blocks until `buffer` is filled. Returns the number of bytes placed
    /// in `buffer`.
    pub async fn read_u16(&mut self, buffer: &mut [u16]) -> Result<usize, Error> {
        let len = dma::receive(&mut self.rx_channel, &self.spi, buffer).await?;
        Ok(len)
    }

    /// Transmit a buffer of bytes to a SPI device
    ///
    /// Blocks until the contents of `buffer` have been transferred from the host controller.
    /// Returns the number of bytes written.
    pub async fn write_u8(&mut self, buffer: &[u8]) -> Result<usize, Error> {
        let len = dma::transfer(&mut self.tx_channel, &self.spi, buffer).await?;
        Ok(len)
    }

    /// Transmit a buffer of `u16`s to a SPI device
    ///
    /// Blocks until the contents of `buffer` have been transferred from the host controller.
    /// Returns the number of bytes written.
    pub async fn write_u16(&mut self, buffer: &[u16]) -> Result<usize, Error> {
        let len = dma::transfer(&mut self.tx_channel, &self.spi, buffer).await?;
        Ok(len)
    }

    /// Transfer bytes from `buffer` while simultaneously receiving bytes into `buffer`
    ///
    /// Each transferred element from the buffer is replaced by an element read from
    /// the SPI device. Returns the number of elements sent and received.
    pub async fn full_duplex_u8(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        // Safety: see safety note in full_duplex_u16
        let (tx, rx) = unsafe {
            futures::future::join(
                dma::receive_raw(
                    &mut self.rx_channel,
                    &self.spi,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                ),
                dma::transfer_raw(
                    &mut self.tx_channel,
                    &self.spi,
                    buffer.as_ptr(),
                    buffer.len(),
                ),
            )
            .await
        };
        let _ = tx?;
        let len = rx?;
        Ok(len)
    }

    /// Transfer `u16` words from `buffer` while simultaneously receiving `u16` words
    /// into `buffer`
    ///
    /// Each transferred element from the buffer is replaced by an element read from
    /// the SPI device. Returns the number of elements sent and received.
    pub async fn full_duplex_u16(&mut self, buffer: &mut [u16]) -> Result<usize, Error> {
        // Safety: the hardware is reading from and writing to the same memory. Even though
        // there is both an immutable and mutable reference to the same memory, software does
        // no observe the memory; it simply passes it down to the hardware.
        //
        // Each SPI receive is dependent on a transfer. The ordering ensures that each element
        // will be transferred out before being overwritten by a received element.
        //
        // Lifetime of buffer exceed lifetime of the DMA future, as observed by the control
        // flow of this function.
        let (tx, rx) = unsafe {
            futures::future::join(
                dma::receive_raw(
                    &mut self.rx_channel,
                    &self.spi,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                ),
                dma::transfer_raw(
                    &mut self.tx_channel,
                    &self.spi,
                    buffer.as_ptr(),
                    buffer.len(),
                ),
            )
            .await
        };
        let _ = tx?;
        let len = rx?;
        Ok(len)
    }
}

/// Must be called while SPI is disabled
fn set_clock_speed(spi: &ral::lpspi::Instance, base: u32, hz: u32) {
    let mut div = base / hz;
    if base / div > hz {
        div += 1;
    }
    let div = div.saturating_sub(2).min(255).max(0);
    ral::write_reg!(
        ral::lpspi,
        spi,
        CCR,
        SCKDIV: div,
        // Both of these delays are arbitrary choices, and they should
        // probably be configurable by the end-user.
        DBT: div / 2,
        SCKPCS: 0x1F,
        PCSSCK: 0x1F
    );
}

/// SPI RX DMA Request signal
///
/// See table 4-3 of the iMXRT1060 Reference Manual (Rev 2)
#[inline(always)]
fn source_signal(spi: &ral::lpspi::Instance) -> u32 {
    #[cfg(not(any(feature = "imxrt1010", feature = "imxrt1060")))]
    compile_error!("Ensure that LPSPI DMAMUX RX channels are correct");

    match &**spi as *const _ {
        // imxrt1010, imxrt1060
        ral::lpspi::LPSPI1 => 13,
        // imxrt1010, imxrt1060
        ral::lpspi::LPSPI2 => 77,
        #[cfg(feature = "imxrt1060")]
        ral::lpspi::LPSPI3 => 15,
        #[cfg(feature = "imxrt1060")]
        ral::lpspi::LPSPI4 => 79,
        _ => unreachable!(),
    }
}

/// SPI TX DMA Request signal
///
/// See table 4-3 of the iMXRT1060 Reference Manual (Rev 2)
#[inline(always)]
fn destination_signal(spi: &ral::lpspi::Instance) -> u32 {
    #[cfg(not(any(feature = "imxrt1010", feature = "imxrt1060")))]
    compile_error!("Ensure that LPSPI DMAMUX TX channels are correct");

    match &**spi as *const _ {
        // imxrt1010, imxrt1060
        ral::lpspi::LPSPI1 => 14,
        // imxrt1010, imxrt1060
        ral::lpspi::LPSPI2 => 78,
        #[cfg(feature = "imxrt1060")]
        ral::lpspi::LPSPI3 => 16,
        #[cfg(feature = "imxrt1060")]
        ral::lpspi::LPSPI4 => 80,
        _ => unreachable!(),
    }
}

/// Adapter to support DMA peripheral traits
/// on RAL LPSPI instances
struct DmaCapable {
    spi: ral::lpspi::Instance,
}

impl core::ops::Deref for DmaCapable {
    type Target = ral::lpspi::Instance;
    fn deref(&self) -> &Self::Target {
        &self.spi
    }
}

#[inline(always)]
fn set_frame_size<Word>(spi: &ral::lpspi::Instance) {
    ral::modify_reg!(ral::lpspi, spi, TCR, FRAMESZ: ((core::mem::size_of::<Word>() * 8 - 1) as u32));
}

#[inline(always)]
fn enable_source<W>(spi: &ral::lpspi::Instance) {
    set_frame_size::<W>(spi);
    ral::modify_reg!(ral::lpspi, spi, FCR, RXWATER: 0); // No watermarks; affects DMA signaling
    ral::modify_reg!(ral::lpspi, spi, DER, RDDE: 1);
}

#[inline(always)]
fn enable_destination<W>(spi: &ral::lpspi::Instance) {
    set_frame_size::<W>(spi);
    ral::modify_reg!(ral::lpspi, spi, FCR, TXWATER: 0); // No watermarks; affects DMA signaling
    ral::modify_reg!(ral::lpspi, spi, DER, TDDE: 1);
}

#[inline(always)]
fn disable_source(spi: &ral::lpspi::Instance) {
    while ral::read_reg!(ral::lpspi, spi, DER, RDDE == 1) {
        ral::modify_reg!(ral::lpspi, spi, DER, RDDE: 0);
    }
}

#[inline(always)]
fn disable_destination(spi: &ral::lpspi::Instance) {
    while ral::read_reg!(ral::lpspi, spi, DER, TDDE == 1) {
        ral::modify_reg!(ral::lpspi, spi, DER, TDDE: 0);
    }
}

unsafe impl dma::Source<u8> for DmaCapable {
    fn source_signal(&self) -> u32 {
        source_signal(&self.spi)
    }
    fn source(&self) -> *const u8 {
        &self.spi.RDR as *const _ as *const u8
    }
    fn enable_source(&self) {
        enable_source::<u8>(&self.spi);
    }
    fn disable_source(&self) {
        disable_source(&self.spi);
    }
}

unsafe impl dma::Destination<u8> for DmaCapable {
    fn destination_signal(&self) -> u32 {
        destination_signal(&self.spi)
    }
    fn destination(&self) -> *const u8 {
        &self.spi.TDR as *const _ as *const u8
    }
    fn enable_destination(&self) {
        enable_destination::<u8>(&self.spi);
    }
    fn disable_destination(&self) {
        disable_destination(&self.spi);
    }
}

unsafe impl dma::Source<u16> for DmaCapable {
    fn source_signal(&self) -> u32 {
        source_signal(&self.spi)
    }
    fn source(&self) -> *const u16 {
        &self.spi.RDR as *const _ as *const u16
    }
    fn enable_source(&self) {
        enable_source::<u16>(&self.spi);
    }
    fn disable_source(&self) {
        disable_source(&self.spi);
    }
}

unsafe impl dma::Destination<u16> for DmaCapable {
    fn destination_signal(&self) -> u32 {
        destination_signal(&self.spi)
    }
    fn destination(&self) -> *const u16 {
        &self.spi.TDR as *const _ as *const u16
    }
    fn enable_destination(&self) {
        enable_destination::<u16>(&self.spi);
    }
    fn disable_destination(&self) {
        disable_destination(&self.spi);
    }
}

/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::ral::{ccm::CCM, lpspi::LPSPI2};
///
/// let hal::ccm::CCM {
///     mut handle,
///     spi_clock,
///     ..
/// } = CCM::take().map(hal::ccm::CCM::from_ral).unwrap();
/// let mut spi_clock = spi_clock.enable(&mut handle);
/// let mut spi2 = LPSPI2::take().unwrap();
/// spi_clock.set_clock_gate(&mut spi2, hal::ccm::ClockGate::On);
/// ```
#[cfg(doctest)]
struct ClockingWeakRalInstance;

/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::ral::{ccm::CCM, lpspi::LPSPI2};
///
/// let hal::ccm::CCM {
///     mut handle,
///     spi_clock,
///     ..
/// } = CCM::take().map(hal::ccm::CCM::from_ral).unwrap();
/// let mut spi_clock = spi_clock.enable(&mut handle);
/// let mut spi2: hal::instance::SPI<hal::iomuxc::consts::U2> = LPSPI2::take()
///     .and_then(hal::instance::spi)
///     .unwrap();
/// spi_clock.set_clock_gate(&mut spi2, hal::ccm::ClockGate::On);
/// ```
#[cfg(doctest)]
struct ClockingStrongHalInstance;
