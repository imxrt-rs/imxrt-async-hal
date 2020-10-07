use crate::{dma, instance, iomuxc, ral};

const CLOCK_DIVIDER: u32 = 5;
/// If changing this, make sure to update `clock`
const CLOCK_HZ: u32 = 528_000_000 / CLOCK_DIVIDER;

const DEFAULT_CLOCK_SPEED_HZ: u32 = 8_000_000;

/// Pins for a SPI device
///
/// Consider using type aliases to simplify your [`SPI`](struct.SPI.html) usage:
///
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::pins;
///
/// // SPI pins used in my application
/// type SPIPins = hal::SPIPins<
///     pins::P11,
///     pins::P12,
///     pins::P13,
///     pins::P10,
/// >;
///
/// // Helper type for your SPI peripheral
/// type SPI = hal::SPI<SPIPins>;
/// ```
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
/// The SPI serial clock speed after construction is unspecified. Use [`set_clock_speed`](#method.set_clock_speed)
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
/// use hal::{dma, instance, iomuxc, t40, SPI, SPIPins};
/// use hal::ral::{
///     ccm::CCM, dma0::DMA0, dmamux::DMAMUX,
///     iomuxc::IOMUXC, lpspi::LPSPI4,
/// };
///
/// let pads = IOMUXC::take().map(iomuxc::new).unwrap();
/// let pins = t40::into_pins(pads);
///
/// let mut ccm = CCM::take().unwrap();
/// let mut channels = dma::channels(
///     DMA0::take().unwrap(),
///     DMAMUX::take().unwrap(),
///     &mut ccm
/// );
///
/// let spi_pins = SPIPins {
///     sdo: pins.p11,
///     sdi: pins.p12,
///     sck: pins.p13,
///     pcs0: pins.p10,
/// };
/// let mut spi4 = LPSPI4::take().and_then(instance::spi).unwrap();
/// let mut spi = SPI::new(
///     spi_pins,
///     spi4,
///     (channels[8].take().unwrap(), channels[9].take().unwrap()),
///     &mut ccm,
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
pub struct SPI<Pins> {
    pins: Pins,
    spi: ral::lpspi::Instance,
    tx_channel: dma::Channel,
    rx_channel: dma::Channel,
}

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
    /// See the [`instance` module](instance/index.html) for more information on SPI peripheral
    /// instances.
    pub fn new(
        mut pins: Pins<SDO, SDI, SCK, PCS0>,
        spi: instance::SPI<M>,
        channels: (dma::Channel, dma::Channel),
        ccm: &mut ral::ccm::Instance,
    ) -> Self {
        enable_clocks(ccm);

        iomuxc::spi::prepare(&mut pins.sdo);
        iomuxc::spi::prepare(&mut pins.sdi);
        iomuxc::spi::prepare(&mut pins.sck);
        iomuxc::spi::prepare(&mut pins.pcs0);

        let spi = spi.release();

        ral::write_reg!(ral::lpspi, spi, CR, RST: RST_1);
        ral::write_reg!(ral::lpspi, spi, CR, RST: RST_0);
        set_clock_speed(&spi, DEFAULT_CLOCK_SPEED_HZ);
        ral::write_reg!(ral::lpspi, spi, CFGR1, MASTER: MASTER_1, SAMPLE: SAMPLE_1);
        // spi.set_mode(embedded_hal::spi::MODE_0).unwrap();
        ral::write_reg!(ral::lpspi, spi, FCR, RXWATER: 0xF, TXWATER: 0xF);
        ral::write_reg!(ral::lpspi, spi, CR, MEN: MEN_1);

        SPI {
            pins,
            spi,
            tx_channel: channels.0,
            rx_channel: channels.1,
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
        (self.pins, self.spi, (self.tx_channel, self.rx_channel))
    }
}

/// Errors propagated from a [`SPI`](struct.SPI.html) device
#[non_exhaustive]
#[derive(Debug)]
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
    /// If an error occurs, it's an [`SPIError::ClockSpeed`](enum.SPIError.html#variant.ClockSpeed).
    pub fn set_clock_speed(&mut self, hz: u32) -> Result<(), Error> {
        self.with_master_disabled(|| {
            // Safety: master is disabled
            set_clock_speed(&self.spi, hz);
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
fn set_clock_speed(spi: &ral::lpspi::Instance, hz: u32) {
    let mut div = CLOCK_HZ / hz;
    if CLOCK_HZ / div > hz {
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

fn enable_clocks(ccm: &mut ral::ccm::Instance) {
    static ONCE: crate::once::Once = crate::once::new();
    ONCE.call(|| {
        // First, disable clocks
        ral::modify_reg!(
            ral::ccm,
            ccm,
            CCGR1,
            CG0: 0,
            CG1: 0,
            CG2: 0,
            CG3: 0
        );

        // Select clock, and commit prescalar
        ral::modify_reg!(
            ral::ccm,
            ccm,
            CBCMR,
            LPSPI_PODF: CLOCK_DIVIDER - 1,
            LPSPI_CLK_SEL: LPSPI_CLK_SEL_2 // PLL2
        );

        // Enable clocks
        ral::modify_reg!(
            ral::ccm,
            ccm,
            CCGR1,
            CG0: 0b11,
            CG1: 0b11,
            CG2: 0b11,
            CG3: 0b11
        );
    });
}

/// SPI RX DMA Request signal
///
/// See table 4-3 of the iMXRT1060 Reference Manual (Rev 2)
#[inline(always)]
fn source_signal(spi: &ral::lpspi::Instance) -> u32 {
    match &**spi as *const _ {
        ral::lpspi::LPSPI1 => 13,
        ral::lpspi::LPSPI2 => 77,
        ral::lpspi::LPSPI3 => 15,
        ral::lpspi::LPSPI4 => 79,
        _ => unreachable!(),
    }
}

/// SPI TX DMA Request signal
///
/// See table 4-3 of the iMXRT1060 Reference Manual (Rev 2)
#[inline(always)]
fn destination_signal(spi: &ral::lpspi::Instance) -> u32 {
    match &**spi as *const _ {
        ral::lpspi::LPSPI1 => 14,
        ral::lpspi::LPSPI2 => 78,
        ral::lpspi::LPSPI3 => 16,
        ral::lpspi::LPSPI4 => 80,
        _ => unreachable!(),
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

impl dma::Source<u8> for ral::lpspi::Instance {
    fn source_signal(&self) -> u32 {
        source_signal(self)
    }
    fn source(&self) -> *const u8 {
        &self.RDR as *const _ as *const u8
    }
    fn enable_source(&self) {
        enable_source::<u8>(self);
    }
    fn disable_source(&self) {
        disable_source(self);
    }
}

impl dma::Destination<u8> for ral::lpspi::Instance {
    fn destination_signal(&self) -> u32 {
        destination_signal(self)
    }
    fn destination(&self) -> *const u8 {
        &self.TDR as *const _ as *const u8
    }
    fn enable_destination(&self) {
        enable_destination::<u8>(self);
    }
    fn disable_destination(&self) {
        disable_destination(self);
    }
}

impl dma::Source<u16> for ral::lpspi::Instance {
    fn source_signal(&self) -> u32 {
        source_signal(self)
    }
    fn source(&self) -> *const u16 {
        &self.RDR as *const _ as *const u16
    }
    fn enable_source(&self) {
        enable_source::<u16>(self);
    }
    fn disable_source(&self) {
        disable_source(self);
    }
}

impl dma::Destination<u16> for ral::lpspi::Instance {
    fn destination_signal(&self) -> u32 {
        destination_signal(self)
    }
    fn destination(&self) -> *const u16 {
        &self.TDR as *const _ as *const u16
    }
    fn enable_destination(&self) {
        enable_destination::<u16>(self);
    }
    fn disable_destination(&self) {
        disable_destination(self);
    }
}
