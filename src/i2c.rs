//! I2C driver, types, and futures
//!
//! The I2C driver utilizes the internal transmit and receive FIFOs to send and
//! receive data. When the transmit buffer is nearly full, the driver yields.
//! When the transmit buffer can support more data, an I2C interrupt
//! fires and wakes the executor. This cycle continues until all of the
//! caller's data is transmitted.
//!
//! When the receive buffer does not have any data, but the caller is awaiting
//! data, the drier yields. Once there's at least one byte in the receive buffer,
//! an I2C interrupt fires and wakers the executor. This cycle continues until
//! all of the caller's receive buffer is filled.
//!
//! The driver also yields when waiting for stop and repeated start conditions.
//!
//! The I2C clock speed is unspecified out of construction. Use [`set_clock_speed`](I2C::set_clock_speed())
//! to select a valid I2C clock speed.
//!
//! The RAL instances are available in `ral::lpi2c`.
//!
//! # Pin configuration
//!
//! You may need to configure the SCL and SDA pins to support your clock speed and data rate. The snippet below
//! provides one possible configuration that supports both 100KHz and 400KHz I2C clock speeds.
//!
//! ```
//! use imxrt_async_hal as hal;
//! use hal::iomuxc;
//!
//! const PINCONFIG: iomuxc::Config = iomuxc::Config::zero()
//!     .set_open_drain(iomuxc::OpenDrain::Enabled)
//!     .set_slew_rate(iomuxc::SlewRate::Fast)
//!     .set_drive_strength(iomuxc::DriveStrength::R0_4)
//!     .set_speed(iomuxc::Speed::Fast)
//!     .set_pull_keep(iomuxc::PullKeep::Enabled)
//!     .set_pull_keep_select(iomuxc::PullKeepSelect::Pull)
//!     .set_pullupdown(iomuxc::PullUpDown::Pullup22k);
//! ```
//!
//! # Example
//!
//! Prepare the I2C3 peripheral at 400KHz. Note that this example does not demonstrate how
//! to set up the I2C peripheral clocks, and enable clock gates.
//!
//! ```no_run
//! use imxrt_async_hal as hal;
//! use hal::{
//!     iomuxc, I2c, I2cClockSpeed,
//!     ral::{self, ccm::CCM, iomuxc::IOMUXC, lpi2c::LPI2C3},
//! };
//! # const PINCONFIG: iomuxc::Config = iomuxc::Config::zero();
//! const SOURCE_CLOCK_HZ: u32 = 24_000_000;
//! const SOURCE_CLOCK_DIVIDER: u32 = 3;
//!
//! let ccm = CCM::take().unwrap();
//! // LPI2C clock selection: 24MHz XTAL, divide by 3
//! ral::modify_reg!(ral::ccm, ccm, CSCDR2, LPI2C_CLK_SEL: 1, LPI2C_CLK_PODF: SOURCE_CLOCK_DIVIDER - 1);
//! // LPI2C3 clock gate on
//! ral::modify_reg!(ral::ccm, ccm, CCGR2, CG5: 0b11);
//!
//! let mut pads = IOMUXC::take()
//!     .map(iomuxc::new)
//!     .unwrap();
//!
//! iomuxc::configure(&mut pads.ad_b1.p07, PINCONFIG);
//! iomuxc::configure(&mut pads.ad_b1.p06, PINCONFIG);
//!
//! let mut i2c3 = LPI2C3::take().unwrap();
//!
//! let mut i2c = I2c::new(i2c3, pads.ad_b1.p07, pads.ad_b1.p06);
//! i2c.set_clock_speed(I2cClockSpeed::KHz400, SOURCE_CLOCK_HZ / SOURCE_CLOCK_DIVIDER).unwrap();
//!
//! # async {
//! # const DEVICE_ADDRESS: u8 = 0;
//! let output = [1, 2, 3, 4];
//! let mut input = [0; 7];
//! i2c.write_read(DEVICE_ADDRESS, &output, &mut input).await.unwrap();
//! # };
//! ```

mod clock;
mod commands;
mod read;
mod write;
mod write_read;

pub use clock::ClockSpeed;
pub use read::Read;
pub use write::Write;
pub use write_read::WriteRead;

use crate::{
    iomuxc,
    ral::{
        self,
        lpi2c::{Instance, RegisterBlock},
    },
};

/// The I2C driver instance
///
/// See the [module-level documentation](mod@crate::i2c) for more information.
#[cfg_attr(docsrs, doc(cfg(feature = "i2c")))]
pub struct I2c<N, SCL, SDA> {
    i2c: Instance<N>,
    scl: SCL,
    sda: SDA,
}

impl<M, SCL, SDA> I2c<M, SCL, SDA>
where
    M: iomuxc::consts::Unsigned,
    SCL: iomuxc::i2c::Pin<Signal = iomuxc::i2c::SCL, Module = M>,
    SDA: iomuxc::i2c::Pin<Signal = iomuxc::i2c::SDA, Module = M>,
{
    /// Create an I2C driver from an I2C instance and a pair of I2C pins
    ///
    /// The I2C clock speed of the returned `I2C` driver is unspecified and may not be valid.
    /// Use [`set_clock_speed`](I2C::set_clock_speed()) to select a valid I2C clock speed.
    pub fn new(i2c: Instance<M>, mut scl: SCL, mut sda: SDA) -> Self {
        iomuxc::i2c::prepare(&mut scl);
        iomuxc::i2c::prepare(&mut sda);

        ral::write_reg!(ral::lpi2c, i2c, MCR, RST: RST_1);
        // Reset is sticky; needs to be explicitly cleared
        ral::write_reg!(ral::lpi2c, i2c, MCR, RST: RST_0);
        ral::write_reg!(ral::lpi2c, i2c, MFCR, TXWATER: 3, RXWATER: 0);
        ral::modify_reg!(ral::lpi2c, i2c, MCR, MEN: MEN_1);

        static ONCE: crate::once::Once = crate::once::new();
        ONCE.call(|| unsafe {
            #[cfg(not(any(feature = "imxrt1010", feature = "imxrt1060")))]
            compile_error!("Ensure that LPI2C interrupts are unmasked");

            // imxrt1010, imxrt1060
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::LPI2C1);
            // imxrt1010, imxrt1060
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::LPI2C2);
            #[cfg(feature = "imxrt1060")]
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::LPI2C3);
            #[cfg(feature = "imxrt1060")]
            cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::LPI2C4);
        });

        I2c { i2c, scl, sda }
    }
}

/// Errors propagated from an [`I2C`] device
#[non_exhaustive]
#[derive(Debug)]
#[cfg_attr(docsrs, doc(cfg(feature = "i2c")))]
pub enum Error {
    /// There was an issue when setting the clock speed
    ///
    /// Only returned from [`set_clock_speed`](I2C::set_clock_speed()).
    ClockSpeed,
    /// Master has lost arbitration
    LostBusArbitration,
    /// SCL and / or SDA went low for too long
    PinLowTimeout,
    /// Detected a NACK when sending an address or data
    UnexpectedNack,
    /// Sending or receiving data without a START condition
    Fifo,
    /// Requesting too much data in a receive
    ///
    /// Upper limit is `u8::max_value()`.
    RequestTooMuchData,
    /// Busy is busy
    ///
    /// The I2C peripheral indicates that it is busy, or that the I2C bus is
    /// busy. Attempting the transaction would block. Consider yielding and
    /// trying again later.
    BusyIsBusy,
}

impl<N, SCL, SDA> I2c<N, SCL, SDA> {
    /// Release the I2C peripheral components
    pub fn release(self) -> (Instance<N>, SCL, SDA) {
        (self.i2c, self.scl, self.sda)
    }

    /// Set the I2C clock speed
    ///
    /// If there is an error, error variant is [`crate::i2c::Error::ClockSpeed`].
    pub fn set_clock_speed(
        &mut self,
        clock_speed: ClockSpeed,
        source_clock_hz: u32,
    ) -> Result<(), Error> {
        while_disabled(&self.i2c, |i2c| {
            clock::set_speed(clock_speed, source_clock_hz, i2c);
        });
        Ok(())
    }

    /// Perform a write-read to an I2C device identified by `address`
    ///
    /// Sends `output`, generates a repeated start, then awaits the I2C device
    /// to send enough data for `input`.
    pub fn write_read<'a>(
        &'a mut self,
        address: u8,
        output: &'a [u8],
        input: &'a mut [u8],
    ) -> write_read::WriteRead<'a> {
        write_read::WriteRead::new(&self.i2c, address, output, input)
    }

    /// Perform an I2C write, sending `buffer` to the I2C device identified by `address`
    pub fn write<'a>(&'a mut self, address: u8, buffer: &'a [u8]) -> write::Write<'a> {
        write::Write::new(&self.i2c, address, buffer)
    }

    /// Request a `buffer` of data from an I2C device identified by `address`
    pub fn read<'a>(&'a mut self, address: u8, buffer: &'a mut [u8]) -> read::Read<'a> {
        read::Read::new(&self.i2c, address, buffer)
    }
}

/// Runs `f` while the I2C peripheral is disabled
///
/// If the peripheral was previously enabled, it will be re-enabled once `while_disabled` returns.
fn while_disabled<F: FnOnce(&RegisterBlock) -> R, R>(i2c: &RegisterBlock, f: F) -> R {
    let was_enabled = ral::read_reg!(ral::lpi2c, i2c, MCR, MEN == MEN_1);
    ral::modify_reg!(ral::lpi2c, i2c, MCR, MEN: MEN_0);
    let result = f(i2c);
    if was_enabled {
        ral::modify_reg!(ral::lpi2c, i2c, MCR, MEN: MEN_1);
    }
    result
}

/// Clears all master status flags that are required to be
/// low before acting as an I2C master.
///
/// All flags are W1C.
#[inline(always)]
fn clear_status(i2c: &RegisterBlock) {
    ral::write_reg!(
        ral::lpi2c,
        i2c,
        MSR,
        EPF: EPF_1,
        SDF: SDF_1,
        NDF: NDF_1,
        ALF: ALF_1,
        FEF: FEF_1,
        PLTF: PLTF_1,
        DMF: DMF_1
    );
}

/// Clear both the receiver and transmit FIFOs
#[inline(always)]
fn clear_fifo(i2c: &RegisterBlock) {
    ral::modify_reg!(ral::lpi2c, i2c, MCR, RRF: RRF_1, RTF: RTF_1);
}

/// Check master status flags for erroneous conditions
#[inline(always)]
fn check_errors(i2c: &RegisterBlock) -> Result<u32, Error> {
    use ral::lpi2c::MSR::*;
    let status = ral::read_reg!(ral::lpi2c, i2c, MSR);
    if (status & PLTF::mask) != 0 {
        Err(Error::PinLowTimeout)
    } else if (status & ALF::mask) != 0 {
        Err(Error::LostBusArbitration)
    } else if (status & NDF::mask) != 0 {
        Err(Error::UnexpectedNack)
    } else if (status & FEF::mask) != 0 {
        Err(Error::Fifo)
    } else {
        Ok(status)
    }
}

/// Returns `true` if the bus is busy, which could block the caller
#[inline(always)]
fn check_busy(i2c: &RegisterBlock) -> Result<(), Error> {
    use ral::lpi2c::MSR;
    let msr = ral::read_reg!(ral::lpi2c, i2c, MSR);
    if (msr & MSR::MBF::mask != 0) || (msr & MSR::BBF::mask != 0) {
        Err(Error::BusyIsBusy)
    } else {
        Ok(())
    }
}

/// Disable the I2C interrupts enabled in `enable_interrupts`
#[inline(always)]
fn disable_interrupts(i2c: &RegisterBlock) {
    ral::write_reg!(
        ral::lpi2c,
        i2c,
        MIER,
        PLTIE: PLTIE_0,
        FEIE: FEIE_1,
        ALIE: ALIE_0,
        NDIE: NDIE_0,
        EPIE: EPIE_0,
        SDIE: SDIE_0,
        RDIE: RDIE_0,
        TDIE: TDIE_0
    );
}

/// I2C polling state
pub enum State {
    StartWrite,
    Send(usize),
    StartRead,
    EndOfPacket,
    ReceiveLength,
    Receive(usize),
    StopSetup,
    Stop,
}
