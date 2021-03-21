//! SPI example that reads the WHO_AM_I register from a connected MPU9250
//!
//! Pinout:
//!
//! - Teensy 4 Pin 13 (SCK) to MPU's SCL (Note that we lose the LED here)
//! - Teensy 4 Pin 11 (MOSI) to MPU's SDA/SDI
//! - Teensy 4 Pin 12 (MISO) to MPU's AD0/SDO
//! - Teensy 4 Pin 10 (PSC0) to MPU's NCS
//!
//! Connect an LED, or monitor pin 14. If everything is working, you should
//! see pin 14 blinking at 2Hz.

#![no_std]
#![no_main]

#[cfg(target_arch = "arm")]
extern crate panic_halt;
#[cfg(target_arch = "arm")]
extern crate t4_startup;

use hal::ral;
use imxrt_async_hal as hal;

const SPI_CLOCK_HZ: u32 = 1_000_000;
/// Effective LPSPI source clock (PLL2)
const SOURCE_CLOCK_HZ: u32 = 528_000_000;
/// Any divider for the source clock
const SOURCE_CLOCK_DIVIDER: u32 = 5;

#[cortex_m_rt::entry]
fn main() -> ! {
    let pads = hal::iomuxc::new(hal::ral::iomuxc::IOMUXC::take().unwrap());
    let pins = teensy4_pins::t40::into_pins(pads);
    let mut hardware_flag = hal::gpio::GPIO::new(pins.p14).output();
    hardware_flag.clear();

    let ccm = hal::ral::ccm::CCM::take().unwrap();
    // Set DMA clock gates to ON
    ral::modify_reg!(ral::ccm, ccm, CCGR5, CG3: 0b11);
    // Enable SPI clocks
    ral::modify_reg!(
        ral::ccm,
        ccm,
        CBCMR,
        LPSPI_CLK_SEL: LPSPI_CLK_SEL_2, /* PLL2 */
        LPSPI_PODF: SOURCE_CLOCK_DIVIDER - 1
    );
    // Unclock SPI4
    ral::modify_reg!(ral::ccm, ccm, CCGR1, CG3: 0b11);
    // DMA clock gate on
    ral::modify_reg!(ral::ccm, ccm, CCGR5, CG3: 0b11);

    let gpt = hal::ral::gpt::GPT2::take().unwrap();

    let (mut timer, _, _) = t4_startup::new_gpt(gpt, &ccm);
    let mut channels = hal::dma::channels(
        hal::ral::dma0::DMA0::take().unwrap(),
        hal::ral::dmamux::DMAMUX::take().unwrap(),
    );

    let spi4 = hal::ral::lpspi::LPSPI4::take()
        .and_then(hal::instance::spi)
        .unwrap();
    let pins = hal::SPIPins {
        sdo: pins.p11,
        sdi: pins.p12,
        sck: pins.p13,
        pcs0: pins.p10,
    };
    let mut spi = hal::SPI::new(
        pins,
        spi4,
        (channels[8].take().unwrap(), channels[9].take().unwrap()),
    );

    spi.set_clock_speed(SPI_CLOCK_HZ, SOURCE_CLOCK_HZ / SOURCE_CLOCK_DIVIDER)
        .unwrap();

    let who_am_i = async {
        loop {
            let mut buffer = [read(WHO_AM_I)];
            let result = spi.full_duplex_u16(&mut buffer).await;
            if result.is_err() || 1 != result.unwrap() || 0x71 != buffer[0] {
                loop {
                    hardware_flag.set();
                }
            }
            t4_startup::gpt_delay_us(&mut timer, 250_000).await;
            hardware_flag.toggle();
        }
    };

    async_embedded::task::block_on(who_am_i);
    unreachable!();
}

/// MPU9250 WHO_AM_I register address
const WHO_AM_I: u8 = 0x75;

/// Creates a read instruction for the MPU9250
const fn read(address: u8) -> u16 {
    ((address as u16) | (1 << 7)) << 8
}
