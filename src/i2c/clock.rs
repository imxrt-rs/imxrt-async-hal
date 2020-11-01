//! I2C clock configuration

use crate::ral::{self, lpi2c::Instance};

/// I2C clock speed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(docsrs, doc(cfg(feature = "i2c")))]
pub enum ClockSpeed {
    /// 100 KHz clock speed
    KHz100,
    /// 400 KHz clock speed
    KHz400,
}

/// Commit the clock speed to the I2C peripheral
///
/// Should only be called while the I2C peripheral is disabled.
pub fn set_speed(clock_speed: ClockSpeed, base_hz: u32, reg: &Instance) {
    // Baud rate = (source_clock/2^prescale)/(CLKLO+1+CLKHI+1 + FLOOR((2+FILTSCL)/2^prescale)
    // Assume CLKLO = 2*CLKHI, SETHOLD = CLKHI, DATAVD = CLKHI/2, FILTSCL = FILTSDA = 0,
    // and that risetime is negligible (less than 1 cycle).
    use core::cmp;
    use ral::lpi2c::MCFGR1::PRESCALE::RW::*;

    const PRESCALARS: [u32; 8] = [
        PRESCALE_0, PRESCALE_1, PRESCALE_2, PRESCALE_3, PRESCALE_4, PRESCALE_5, PRESCALE_6,
        PRESCALE_7,
    ];

    struct ByError {
        prescalar: u32,
        clkhi: u32,
        error: u32,
    }

    let baud_rate: u32 = match clock_speed {
        ClockSpeed::KHz100 => 100_000,
        ClockSpeed::KHz400 => 400_000,
    };

    // prescale = 1, 2, 4, 8, ... 128
    // divider = 2 ^ prescale
    let dividers = PRESCALARS.iter().copied().map(|prescalar| 1 << prescalar);
    let clkhis = 1u32..32u32;
    // possibilities = every divider with every clkhi (8 * 30 == 240 possibilities)
    let possibilities =
        dividers.flat_map(|divider| core::iter::repeat(divider).zip(clkhis.clone()));
    let errors = possibilities.map(|(divider, clkhi)| {
        let computed_rate = if 1 == clkhi {
            // See below for justification on magic numbers.
            // In the 1 == clkhi case, the + 3 is the minimum allowable CLKLO value
            // + 1 is CLKHI itself
            (base_hz / divider) / ((1 + 3 + 2) + 2 / divider)
        } else {
            // CLKLO = 2 * CLKHI, allows us to do 3 * CLKHI
            // + 2 accounts for the CLKLOW + 1 and CLKHI + 1
            // + 2 accounts for the FLOOR((2 + FILTSCL)) factor
            (base_hz / divider) / ((3 * clkhi + 2) + 2 / divider)
        };
        let error = cmp::max(computed_rate, baud_rate) - cmp::min(computed_rate, baud_rate);
        ByError {
            prescalar: divider.saturating_sub(1).count_ones(),
            clkhi, /* (1..32) in u8 range */
            error,
        }
    });

    let ByError {
        prescalar, clkhi, ..
    } = errors.min_by(|lhs, rhs| lhs.error.cmp(&rhs.error)).unwrap();

    let (clklo, sethold, datavd) = if clkhi < 2 {
        (3, 2, 1)
    } else {
        (clkhi * 2, clkhi, clkhi / 2)
    };

    ral::write_reg!(
        ral::lpi2c,
        reg,
        MCCR0,
        CLKHI: clkhi,
        CLKLO: clklo,
        SETHOLD: sethold,
        DATAVD: datavd
    );
    ral::write_reg!(ral::lpi2c, reg, MCFGR1, PRESCALE: prescalar);
}
