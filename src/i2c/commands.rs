//! Asynchornous I2C commands
//!
//! This implements the async logic for I2C I/O.
//!
//! # Developer notes
//!
//! MIER documentation indicates that FEIE is 0 for 'enable' and 1 for 'disable',
//! which is the inverse of every other configuration bit... Not sure if it's a
//! documentation error, or if that's by design. We're trusting the documentation
//! here, and we'll turn it off. The implementation will check for a FIFO error
//! while clocking-out data.

use super::Error;
use crate::ral::{self, lpi2c::RegisterBlock};

use core::{
    sync::atomic,
    task::{Context, Poll, Waker},
};

/// Resolves when there's space in the transmit FIFO
fn poll_transmit_ready(i2c: &RegisterBlock, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
    if let Err(err) = super::check_errors(&i2c) {
        Poll::Ready(Err(err))
    } else if ral::read_reg!(ral::lpi2c, i2c, MSR, TDF == TDF_1) {
        Poll::Ready(Ok(()))
    } else {
        *waker(&i2c) = Some(cx.waker().clone());
        atomic::compiler_fence(atomic::Ordering::Release);
        enable_interrupts(&i2c, InterruptKind::Transfer);
        Poll::Pending
    }
}

/// Command a write to `address`
pub fn poll_start_write(
    i2c: &RegisterBlock,
    cx: &mut Context<'_>,
    address: u8,
) -> Poll<Result<(), Error>> {
    poll_transmit_ready(i2c, cx).map_ok(|_| {
        ral::write_reg!(ral::lpi2c, i2c, MTDR, CMD: CMD_4, DATA: (address as u32) << 1);
    })
}

/// Command a read from `address`
pub fn poll_start_read(
    i2c: &RegisterBlock,
    cx: &mut Context<'_>,
    address: u8,
) -> Poll<Result<(), Error>> {
    poll_transmit_ready(i2c, cx).map_ok(|_| {
        ral::write_reg!(ral::lpi2c, i2c, MTDR, CMD: CMD_4, DATA: ((address as u32) << 1) | 1);
    })
}

/// Send `value` to the I2C device
pub fn poll_send(i2c: &RegisterBlock, cx: &mut Context<'_>, value: u8) -> Poll<Result<(), Error>> {
    poll_transmit_ready(i2c, cx).map_ok(|_| {
        ral::write_reg!(ral::lpi2c, i2c, MTDR, CMD: CMD_0, DATA: value as u32);
    })
}

/// Resolves when we acknowledge and end of packet (repeated start, or stop condition)
pub fn poll_end_of_packet(i2c: &RegisterBlock, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
    if let Err(err) = super::check_errors(&i2c) {
        Poll::Ready(Err(err))
    } else if ral::read_reg!(ral::lpi2c, i2c, MSR, EPF == EPF_1) {
        // W1C
        ral::modify_reg!(ral::lpi2c, i2c, MSR, EPF: EPF_1);
        Poll::Ready(Ok(()))
    } else {
        *waker(&i2c) = Some(cx.waker().clone());
        atomic::compiler_fence(atomic::Ordering::Release);
        enable_interrupts(&i2c, InterruptKind::EndPacket);
        Poll::Pending
    }
}

/// Prepare to receive `len` bytes from the I2C device
pub fn poll_receive_length(
    i2c: &RegisterBlock,
    cx: &mut Context<'_>,
    len: usize,
) -> Poll<Result<(), Error>> {
    poll_transmit_ready(i2c, cx)
        .map_ok(|_| ral::write_reg!(ral::lpi2c, i2c, MTDR, CMD: CMD_1, DATA: (len - 1) as u32))
}

/// Receive a byte from the I2C device
pub fn poll_receive(i2c: &RegisterBlock, cx: &mut Context<'_>) -> Poll<Result<u8, Error>> {
    if let Err(err) = super::check_errors(&i2c) {
        Poll::Ready(Err(err))
    } else if ral::read_reg!(ral::lpi2c, i2c, MSR, RDF == RDF_1) {
        let byte = ral::read_reg!(ral::lpi2c, i2c, MRDR, DATA);
        Poll::Ready(Ok(byte as u8))
    } else {
        *waker(&i2c) = Some(cx.waker().clone());
        atomic::compiler_fence(atomic::Ordering::Release);
        enable_interrupts(&i2c, InterruptKind::Receive);
        Poll::Pending
    }
}

/// Command a stop, resolving once the command is enqueued
pub fn poll_stop_setup(i2c: &RegisterBlock, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
    poll_transmit_ready(i2c, cx).map_ok(|_| {
        ral::write_reg!(ral::lpi2c, i2c, MTDR, CMD: CMD_2);
    })
}

/// Resolves when the stop condition generates an interrupt
pub fn poll_stop(i2c: &RegisterBlock, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
    if let Err(err) = super::check_errors(&i2c) {
        Poll::Ready(Err(err))
    } else if ral::read_reg!(ral::lpi2c, i2c, MSR, SDF == SDF_1) {
        // W1C
        ral::modify_reg!(ral::lpi2c, i2c, MSR, SDF: SDF_1);
        Poll::Ready(Ok(()))
    } else {
        *waker(&i2c) = Some(cx.waker().clone());
        atomic::compiler_fence(atomic::Ordering::Release);
        enable_interrupts(&i2c, InterruptKind::Stop);
        Poll::Pending
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InterruptKind {
    Receive,
    Transfer,
    EndPacket,
    Stop,
}

/// Enable the I2C interrupts of interest
#[inline(always)]
fn enable_interrupts(i2c: &RegisterBlock, kind: InterruptKind) {
    ral::write_reg!(
        ral::lpi2c,
        i2c,
        MIER,
        PLTIE: PLTIE_1,
        FEIE: FEIE_0,
        ALIE: ALIE_1,
        NDIE: NDIE_1,
        EPIE: (kind == InterruptKind::EndPacket) as u32,
        SDIE: (kind == InterruptKind::Stop) as u32,
        RDIE: (kind == InterruptKind::Receive) as u32,
        TDIE: (kind == InterruptKind::Transfer) as u32
    );
}

#[inline(always)]
fn on_interrupt(i2c: &RegisterBlock) {
    super::disable_interrupts(i2c);
    if let Some(waker) = waker(i2c).take() {
        waker.wake();
    }
}

/// Returns the waker state associated with this I2C instance
fn waker(i2c: &RegisterBlock) -> &'static mut Option<Waker> {
    static mut WAKERS: [Option<Waker>; 4] = [None, None, None, None];
    let inst = match &*i2c as *const _ {
        ral::lpi2c::LPI2C1 => 0,
        ral::lpi2c::LPI2C2 => 1,
        #[cfg(feature = "imxrt1060")]
        ral::lpi2c::LPI2C3 => 2,
        #[cfg(feature = "imxrt1060")]
        ral::lpi2c::LPI2C4 => 3,
        _ => unreachable!(),
    };
    unsafe { &mut WAKERS[inst] }
}

#[cfg(not(any(feature = "imxrt1010", feature = "imxrt1060")))]
compile_error!("Ensure that LPI2C interrupts are correctly defined");
interrupts! {
    handler!{unsafe fn LPI2C1() {
        on_interrupt(&ral::lpi2c::LPI2C1::steal());
    }}


    handler!{unsafe fn LPI2C2() {
        on_interrupt(&ral::lpi2c::LPI2C2::steal());
    }}

    #[cfg(feature = "imxrt1060")]
    handler!{unsafe fn LPI2C3() {
        on_interrupt(&ral::lpi2c::LPI2C3::steal());
    }}

    #[cfg(feature = "imxrt1060")]
    handler!{unsafe fn LPI2C4() {
        on_interrupt(&ral::lpi2c::LPI2C4::steal());
    }}
}
