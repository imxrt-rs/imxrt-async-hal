use crate::{dma, iomuxc, ral};

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
/// use hal::{dma, instance, iomuxc, SPI, SPIPins};
/// use hal::ral::{self,
///     ccm::CCM, dma0::DMA0, dmamux::DMAMUX,
///     iomuxc::IOMUXC, lpspi::LPSPI4,
/// };
///
/// // Effective LPSPI source clock (PLL2)
/// const SOURCE_CLOCK_HZ: u32 = 528_000_000;
/// // Any divider for the source clock
/// const SOURCE_CLOCK_DIVIDER: u32 = 5;
///
/// let pads = IOMUXC::take().map(iomuxc::new).unwrap();
///
/// let ccm = CCM::take().unwrap();
/// // Prepare SPI clocks
/// ral::modify_reg!(
///     ral::ccm,
///     ccm,
///     CBCMR,
///     LPSPI_CLK_SEL: LPSPI_CLK_SEL_2, /* PLL2 */
///     LPSPI_PODF: SOURCE_CLOCK_DIVIDER - 1
/// );
/// // LPSPI4 clock gate on
/// ral::modify_reg!(ral::ccm, ccm, CCGR1, CG3: 0b11);
/// // DMA clock on
/// ral::modify_reg!(ral::ccm, ccm, CCGR5, CG3: 0b11);
///
/// let dma = DMA0::take().unwrap();
/// let mut channels = dma::channels(
///     dma,
///     DMAMUX::take().unwrap(),
/// );
///
/// let spi_pins = SPIPins {
///     sdo: pads.b0.p02,
///     sdi: pads.b0.p01,
///     sck: pads.b0.p03,
///     pcs0: pads.b0.p00,
/// };
/// let spi4 = LPSPI4::take().and_then(instance::spi).unwrap();
/// let mut spi = SPI::new(
///     spi_pins,
///     spi4,
/// );
///
/// let mut tx_channel = channels[8].take().unwrap();
/// tx_channel.set_interrupt_on_completion(true);
/// let mut rx_channel = channels[9].take().unwrap();
/// rx_channel.set_interrupt_on_completion(true);
///
/// spi.set_clock_speed(1_000_000, SOURCE_CLOCK_HZ / SOURCE_CLOCK_DIVIDER).unwrap();
///
/// # async {
/// let mut buffer = [1u16, 2, 3, 4];
/// // Transmit the u16 words in buffer, and receive the reply into buffer.
/// spi.dma_full_duplex(&mut rx_channel, &mut tx_channel, &mut buffer).await;
/// # };
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
pub struct SPI<N, Pins> {
    pins: Pins,
    spi: ral::lpspi::Instance<N>,
}

impl<SDO, SDI, SCK, PCS0, M> SPI<M, Pins<SDO, SDI, SCK, PCS0>>
where
    SDO: iomuxc::spi::Pin<Module = M, Signal = iomuxc::spi::SDO>,
    SDI: iomuxc::spi::Pin<Module = M, Signal = iomuxc::spi::SDI>,
    SCK: iomuxc::spi::Pin<Module = M, Signal = iomuxc::spi::SCK>,
    PCS0: iomuxc::spi::Pin<Module = M, Signal = iomuxc::spi::PCS0>,
    M: iomuxc::consts::Unsigned,
{
    /// Create a `SPI` from a set of pins and a SPI instance
    ///
    /// See the [`instance` module](instance) for more information on SPI peripheral
    /// instances.
    ///
    /// The clock speed is unspecified. Make sure you change your clock speed with `set_clock_speed`.
    pub fn new(mut pins: Pins<SDO, SDI, SCK, PCS0>, spi: ral::lpspi::Instance<M>) -> Self {
        iomuxc::spi::prepare(&mut pins.sdo);
        iomuxc::spi::prepare(&mut pins.sdi);
        iomuxc::spi::prepare(&mut pins.sck);
        iomuxc::spi::prepare(&mut pins.pcs0);

        ral::write_reg!(ral::lpspi, spi, CR, RST: RST_1);
        ral::write_reg!(ral::lpspi, spi, CR, RST: RST_0);
        ral::write_reg!(ral::lpspi, spi, CFGR1, MASTER: MASTER_1, SAMPLE: SAMPLE_1);
        // spi.set_mode(embedded_hal::spi::MODE_0).unwrap();
        ral::write_reg!(ral::lpspi, spi, FCR, RXWATER: 0xF, TXWATER: 0xF);
        ral::write_reg!(ral::lpspi, spi, CR, MEN: MEN_1);

        SPI { pins, spi }
    }
}

impl<Pins, M> SPI<M, Pins> {
    /// Return the pins and SPI instance that are used in this `SPI`
    /// driver
    pub fn release(self) -> (Pins, ral::lpspi::Instance<M>) {
        (self.pins, self.spi)
    }

    fn set_frame_size<W>(&mut self) {
        ral::modify_reg!(ral::lpspi, self.spi, TCR, FRAMESZ: ((core::mem::size_of::<W>() * 8 - 1) as u32));
    }

    /// Use a DMA channel to read data from the SPI peripheral
    pub fn dma_read<'a, E: dma::Element>(
        &'a mut self,
        channel: &'a mut dma::Channel,
        buffer: &'a mut [E],
    ) -> dma::Rx<'a, Self, E> {
        dma::receive(channel, self, buffer)
    }

    /// Use a DMA channel to write data to the SPI peripheral
    pub fn dma_write<'a, E: dma::Element>(
        &'a mut self,
        channel: &'a mut dma::Channel,
        buffer: &'a [E],
    ) -> dma::Tx<'a, Self, E> {
        dma::transfer(channel, buffer, self)
    }

    /// Use two DMA channels to perform a full-duplex transfer
    pub fn dma_full_duplex<'a, E: dma::Element>(
        &'a mut self,
        rx_channel: &'a mut dma::Channel,
        tx_channel: &'a mut dma::Channel,
        buffer: &'a mut [E],
    ) -> dma::FullDuplex<'a, Self, E> {
        dma::full_duplex(rx_channel, tx_channel, self, buffer)
    }
}

/// Errors propagated from a [`SPI`] device
#[non_exhaustive]
#[derive(Debug)]
#[cfg_attr(docsrs, doc(cfg(feature = "spi")))]
pub enum Error {
    /// Error when configuring the SPI serial clock
    ClockSpeed,
}

impl<N, Pins> SPI<N, Pins> {
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
    pub fn set_clock_speed(&mut self, hz: u32, source_clock_hz: u32) -> Result<(), Error> {
        self.with_master_disabled(|| {
            // Safety: master is disabled
            set_clock_speed(&self.spi, source_clock_hz, hz);
            Ok(())
        })
    }
}

/// Must be called while SPI is disabled
fn set_clock_speed(spi: &ral::lpspi::RegisterBlock, base: u32, hz: u32) {
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

unsafe impl<E: dma::Element, Pins, N> dma::Source<E> for SPI<N, Pins> {
    fn source_signal(&self) -> u32 {
        match &*self.spi as *const _ {
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
    fn source_address(&self) -> *const E {
        &self.spi.RDR as *const _ as *const E
    }
    fn enable_source(&mut self) {
        self.set_frame_size::<E>();
        ral::modify_reg!(ral::lpspi, self.spi, FCR, RXWATER: 0);
        ral::modify_reg!(ral::lpspi, self.spi, DER, RDDE: 1);
    }
    fn disable_source(&mut self) {
        while ral::read_reg!(ral::lpspi, self.spi, DER, RDDE == 1) {
            ral::modify_reg!(ral::lpspi, self.spi, DER, RDDE: 0);
        }
    }
}

unsafe impl<E: dma::Element, Pins, N> dma::Destination<E> for SPI<N, Pins> {
    fn destination_signal(&self) -> u32 {
        <Self as dma::Source<E>>::source_signal(self) + 1
    }
    fn destination_address(&self) -> *const E {
        &self.spi.TDR as *const _ as *const E
    }
    fn enable_destination(&mut self) {
        self.set_frame_size::<E>();
        ral::modify_reg!(ral::lpspi, self.spi, FCR, TXWATER: 0);
        ral::modify_reg!(ral::lpspi, self.spi, DER, TDDE: 1);
    }
    fn disable_destination(&mut self) {
        while ral::read_reg!(ral::lpspi, self.spi, DER, TDDE == 1) {
            ral::modify_reg!(ral::lpspi, self.spi, DER, TDDE: 0);
        }
    }
}

unsafe impl<E: dma::Element, Pins, N> dma::Bidirectional<E> for SPI<N, Pins> {}
