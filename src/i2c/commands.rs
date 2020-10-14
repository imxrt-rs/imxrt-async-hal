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
use crate::{
    instance::Inst,
    ral::{self, lpi2c::Instance},
};

use core::{
    future::Future,
    pin::Pin,
    sync::atomic,
    task::{Context, Poll, Waker},
};

/// Future that awaits space in the transmit FIFO
struct TransmitReady<'t>(&'t Instance);

impl<'t> Future for TransmitReady<'t> {
    type Output = Result<(), Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Err(err) = super::check_errors(&self.0) {
            Poll::Ready(Err(err))
        } else if ral::read_reg!(ral::lpi2c, self.0, MSR, TDF == TDF_1) {
            Poll::Ready(Ok(()))
        } else {
            *waker(&self.0) = Some(cx.waker().clone());
            atomic::compiler_fence(atomic::Ordering::Release);
            enable_interrupts(&self.0, InterruptKind::Transfer);
            Poll::Pending
        }
    }
}

impl<'t> Drop for TransmitReady<'t> {
    fn drop(&mut self) {
        disable_interrupts(&self.0);
    }
}

/// Generates a (repeat) start condition for a write to the device with `address`
#[inline(always)]
pub async fn start_write(i2c: &Instance, address: u8) -> Result<(), Error> {
    TransmitReady(i2c).await?;
    ral::write_reg!(ral::lpi2c, i2c, MTDR, CMD: CMD_4, DATA: (address as u32) << 1);
    Ok(())
}

/// Generates a (repeat) start condition for a read from the device with `address`
#[inline(always)]
pub async fn start_read(i2c: &Instance, address: u8) -> Result<(), Error> {
    TransmitReady(i2c).await?;
    ral::write_reg!(ral::lpi2c, i2c, MTDR, CMD: CMD_4, DATA: ((address as u32) << 1) | 1);
    Ok(())
}

/// Send `buffer` to the previously-addressed I2C device
#[inline(always)]
pub async fn send(i2c: &Instance, buffer: &[u8]) -> Result<(), Error> {
    for byte in buffer {
        TransmitReady(i2c).await?;
        ral::write_reg!(ral::lpi2c, i2c, MTDR, CMD: CMD_0, DATA: *byte as u32);
    }
    Ok(())
}

/// Awaits an end of packet signal
///
/// End of packet signals either a repeated start, or a stop condition.
#[inline(always)]
pub async fn end_of_packet(i2c: &Instance) -> Result<(), Error> {
    struct EndOfPacket<'t>(&'t Instance);
    impl<'t> Future for EndOfPacket<'t> {
        type Output = Result<(), Error>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if let Err(err) = super::check_errors(&self.0) {
                Poll::Ready(Err(err))
            } else if ral::read_reg!(ral::lpi2c, self.0, MSR, EPF == EPF_1) {
                // W1C
                ral::modify_reg!(ral::lpi2c, self.0, MSR, EPF: EPF_1);
                Poll::Ready(Ok(()))
            } else {
                *waker(&self.0) = Some(cx.waker().clone());
                atomic::compiler_fence(atomic::Ordering::Release);
                enable_interrupts(&self.0, InterruptKind::EndPacket);
                Poll::Pending
            }
        }
    };
    impl<'t> Drop for EndOfPacket<'t> {
        fn drop(&mut self) {
            disable_interrupts(&self.0);
        }
    }

    EndOfPacket(i2c).await
}

/// A future that yields a byte from the receive FIFO
struct Receive<'t>(&'t Instance);

impl<'t> Future for Receive<'t> {
    type Output = Result<u8, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Err(err) = super::check_errors(&self.0) {
            Poll::Ready(Err(err))
        } else if ral::read_reg!(ral::lpi2c, self.0, MSR, RDF == RDF_1) {
            let byte = ral::read_reg!(ral::lpi2c, self.0, MRDR, DATA);
            Poll::Ready(Ok(byte as u8))
        } else {
            *waker(&self.0) = Some(cx.waker().clone());
            atomic::compiler_fence(atomic::Ordering::Release);
            enable_interrupts(&self.0, InterruptKind::Receive);
            Poll::Pending
        }
    }
}

impl<'t> Drop for Receive<'t> {
    fn drop(&mut self) {
        disable_interrupts(&self.0);
    }
}

/// Await to receive a buffer of data
#[inline(always)]
pub async fn receive(i2c: &Instance, buffer: &mut [u8]) -> Result<(), Error> {
    TransmitReady(i2c).await?;
    ral::write_reg!(ral::lpi2c, i2c, MTDR, CMD: CMD_1, DATA: (buffer.len() - 1) as u32);
    for byte in buffer.iter_mut() {
        *byte = Receive(i2c).await?;
    }
    Ok(())
}

/// Generate a stop condition, and await the host to send the stop condition
#[inline(always)]
pub async fn stop(i2c: &Instance) -> Result<(), Error> {
    TransmitReady(i2c).await?;
    ral::write_reg!(ral::lpi2c, i2c, MTDR, CMD: CMD_2);

    struct Stop<'t>(&'t Instance);
    impl<'t> Future for Stop<'t> {
        type Output = Result<(), Error>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if let Err(err) = super::check_errors(&self.0) {
                Poll::Ready(Err(err))
            } else if ral::read_reg!(ral::lpi2c, self.0, MSR, SDF == SDF_1) {
                // W1C
                ral::modify_reg!(ral::lpi2c, self.0, MSR, SDF: SDF_1);
                Poll::Ready(Ok(()))
            } else {
                *waker(&self.0) = Some(cx.waker().clone());
                atomic::compiler_fence(atomic::Ordering::Release);
                enable_interrupts(&self.0, InterruptKind::Stop);
                Poll::Pending
            }
        }
    }
    impl<'t> Drop for Stop<'t> {
        fn drop(&mut self) {
            disable_interrupts(&self.0);
        }
    }

    Stop(i2c).await
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
fn enable_interrupts(i2c: &Instance, kind: InterruptKind) {
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

/// Disable the I2C interrupts enabled in `enable_interrupts`
#[inline(always)]
fn disable_interrupts(i2c: &Instance) {
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

#[inline(always)]
fn on_interrupt(i2c: &Instance) {
    disable_interrupts(i2c);
    if let Some(waker) = waker(i2c).take() {
        waker.wake();
    }
}

/// Returns the waker state associated with this I2C instance
fn waker(i2c: &Instance) -> &'static mut Option<Waker> {
    static mut WAKERS: [Option<Waker>; 4] = [None, None, None, None];
    unsafe { &mut WAKERS[i2c.inst().wrapping_sub(1)] }
}

#[cfg(not(any(feature = "imxrt101x", feature = "imxrt106x")))]
compile_error!("Ensure that LPI2C interrupts are correctly defined");
interrupts! {
    handler!{unsafe fn LPI2C1() {
        on_interrupt(&ral::lpi2c::LPI2C1::steal());
    }}


    handler!{unsafe fn LPI2C2() {
        on_interrupt(&ral::lpi2c::LPI2C2::steal());
    }}

    #[cfg(feature = "imxrt106x")]
    handler!{unsafe fn LPI2C3() {
        on_interrupt(&ral::lpi2c::LPI2C3::steal());
    }}

    #[cfg(feature = "imxrt106x")]
    handler!{unsafe fn LPI2C4() {
        on_interrupt(&ral::lpi2c::LPI2C4::steal());
    }}
}
