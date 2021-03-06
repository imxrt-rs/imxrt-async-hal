//! Example of an asynchronous I2C
//!
//! Teensy pin 16 => SCL (I2C3)
//! Teensy pin 17 => SDA (I2C3)
//!
//! Success criteria:
//!
//! - The MPU correctly reports its `WHO_AM_I` address. The slave
//!   address is printed over USB logging.
//! - The clock is running at its selected bit rate; either 100KHz
//!   or 400KHz. Measure it with a logic analyzer.
//! - There's a repeated start in the `write_read` call; observable
//!   via a logic analyzer. Show that a `write`, followed by a
//!   `read`, uses two transactions.

#![no_std]
#![no_main]

#[cfg(target_arch = "arm")]
extern crate panic_halt;
#[cfg(target_arch = "arm")]
extern crate t4_startup;

use hal::{
    gpio::GPIO,
    iomuxc,
    ral::{self, ccm::CCM, gpt::GPT1, iomuxc::IOMUXC, lpi2c::LPI2C3},
    I2C,
};
use imxrt_async_hal as hal;

const MPU9250_ADDRESS: u8 = 0x68;
const WHO_AM_I: u8 = 0x75;
const ACCEL_XOUT_H: u8 = 0x3B;
const CLOCK_SPEED: hal::I2CClockSpeed = hal::I2CClockSpeed::KHz400;

const SOURCE_CLOCK_HZ: u32 = 24_000_000;
const SOURCE_CLOCK_DIVIDER: u32 = 3;

const PINCONFIG: iomuxc::Config = iomuxc::Config::zero()
    .set_open_drain(iomuxc::OpenDrain::Enabled)
    .set_slew_rate(iomuxc::SlewRate::Fast)
    .set_drive_strength(iomuxc::DriveStrength::R0_4)
    .set_speed(iomuxc::Speed::Fast)
    .set_pull_keep(iomuxc::PullKeep::Enabled)
    .set_pull_keep_select(iomuxc::PullKeepSelect::Pull)
    .set_pullupdown(iomuxc::PullUpDown::Pullup22k);

#[cortex_m_rt::entry]
fn main() -> ! {
    let mut pins = IOMUXC::take()
        .map(hal::iomuxc::new)
        .map(teensy4_pins::t40::into_pins)
        .unwrap();

    iomuxc::configure(&mut pins.p16, PINCONFIG);
    iomuxc::configure(&mut pins.p17, PINCONFIG);

    let ccm = CCM::take().unwrap();
    ral::modify_reg!(ral::ccm, ccm, CSCDR2, LPI2C_CLK_SEL: 1, LPI2C_CLK_PODF: SOURCE_CLOCK_DIVIDER - 1);
    ral::modify_reg!(ral::ccm, ccm, CCGR2, CG5: 0b11);
    let (mut timer, _, _) = t4_startup::new_gpt(GPT1::take().unwrap(), &ccm);

    let mut led = GPIO::new(pins.p13).output();

    let i2c3 = LPI2C3::take().and_then(hal::instance::i2c).unwrap();
    let mut i2c = I2C::new(i2c3, pins.p16, pins.p17);
    i2c.set_clock_speed(CLOCK_SPEED, SOURCE_CLOCK_HZ / SOURCE_CLOCK_DIVIDER)
        .unwrap();

    let task = async {
        loop {
            // Note: the write, then read, for WHO_AM_I could be achieved in a single write_read.
            // The separation here is intentional, so that we can test the driver.
            // We're also reading more data than WHO_AM_I would actually return.
            let mut input = [0u8; 2];
            if i2c.write(MPU9250_ADDRESS, &[WHO_AM_I]).await.is_err() {
                loop {
                    led.toggle();
                    t4_startup::gpt_delay_us(&mut timer, 1_000_000).await;
                }
            }
            t4_startup::gpt_delay_us(&mut timer, 1_000).await;
            if i2c.read(MPU9250_ADDRESS, &mut input).await.is_err() || input[0] != 0x71 {
                loop {
                    led.toggle();
                    t4_startup::gpt_delay_us(&mut timer, 1_000_000).await;
                }
            }

            led.toggle();
            t4_startup::gpt_delay_us(&mut timer, 250_000).await;

            let mut buffer = [0u8; 14];
            if i2c
                .write_read(MPU9250_ADDRESS, &[ACCEL_XOUT_H], &mut buffer)
                .await
                .is_err()
            {
                loop {
                    led.toggle();
                    t4_startup::gpt_delay_us(&mut timer, 1_000_000).await;
                }
            }

            led.toggle();
            t4_startup::gpt_delay_us(&mut timer, 250_000).await;
        }
    };
    async_embedded::task::block_on(task);
    unreachable!();
}
