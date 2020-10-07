//! Supporting traits for defining peripheral DMA
//! sources and destinations

use super::{element::Element, interrupt, Channel, Error};

/// Describes a peripheral that can be the source of DMA data
///
/// By 'source,' we mean that it provides data for a DMA transfer.
/// A 'source,' would be a hardware device sending data into our
/// memory.
pub trait Source<E: Element> {
    /// Peripheral source request signal
    ///
    /// See Table 4-3 of the reference manual. A source probably
    /// has something like 'receive' in the name.
    fn source_signal(&self) -> u32;
    /// Returns a pointer to the register from which the DMA channel
    /// reads data
    ///
    /// This is the register that software reads to acquire data from
    /// a device. The type of the pointer describes the type of reads
    /// the DMA channel performs when transferring data.
    ///
    /// This memory is assumed to be static.
    fn source(&self) -> *const E;
    /// Perform any actions necessary to enable DMA transfers
    ///
    /// Callers use this method to put the peripheral in a state where
    /// it can supply the DMA channel with data.
    fn enable_source(&self);
    /// Perform any actions necessary to disable or cancel DMA transfers
    ///
    /// This may include undoing the actions in `enable_source()`.
    fn disable_source(&self);
}

/// Describes a peripheral that can be the destination for DMA data
///
/// By 'destination,' we mean that it receives data from a DMA transfer.
/// Software is sending data from memory to a device using DMA.
pub trait Destination<E: Element> {
    /// Peripheral destination request signal
    ///
    /// See Table 4-3 of the reference manual. A destination probably
    /// has something like 'transfer' in the name.
    fn destination_signal(&self) -> u32;
    /// Returns a pointer to the register into which the DMA channel
    /// writes data
    ///
    /// This is the register that software writes to when sending data to a
    /// device. The type of the pointer describes the type of reads the
    /// DMA channel performs when transferring data.
    fn destination(&self) -> *const E;
    /// Perform any actions necessary to enable DMA transfers
    ///
    /// Callers use this method to put the peripheral into a state where
    /// it can accept transfers from a DMA channel.
    fn enable_destination(&self);
    /// Perform any actions necessary to disable or cancel DMA transfers
    ///
    /// This may include undoing the actions in `enable_destination()`.
    fn disable_destination(&self);
}

pub type Result<T> = core::result::Result<T, Error>;

pub async fn receive<S, E>(channel: &mut Channel, source: &S, buffer: &mut [E]) -> Result<usize>
where
    S: Source<E>,
    E: Element,
{
    channel.set_trigger_from_hardware(Some(source.source_signal()));
    unsafe {
        channel.set_source_transfer(super::Transfer::hardware(source.source()));
    }
    let buffer_description = super::Transfer::buffer_mut(buffer);
    unsafe {
        channel.set_destination_transfer(buffer_description);
    }
    channel.set_minor_loop_elements::<E>(1);
    channel.set_transfer_iterations(buffer.len() as u16);
    source.enable_source();
    let on_drop = |channel: &mut Channel| {
        source.disable_source();
        while channel.is_hardware_signaling() {}
    };
    unsafe { interrupt::interrupt(channel, on_drop) }.await?;
    Ok(buffer.len())
}

/// # Safety
///
/// Caller must ensure that `data` outlives, and remains valid for, the lifetime
/// of the DMA operation.
pub async unsafe fn receive_raw<S, E>(
    channel: &mut Channel,
    source: &S,
    data: *mut E,
    len: usize,
) -> Result<usize>
where
    S: Source<E>,
    E: Element,
{
    receive(channel, source, core::slice::from_raw_parts_mut(data, len)).await
}

pub async fn transfer<D, E>(channel: &mut Channel, destination: &D, buffer: &[E]) -> Result<usize>
where
    D: Destination<E>,
    E: Element,
{
    channel.set_trigger_from_hardware(Some(destination.destination_signal()));
    unsafe {
        channel.set_destination_transfer(super::Transfer::hardware(destination.destination()));
    }
    let buffer_description = super::Transfer::buffer(buffer);
    unsafe {
        channel.set_source_transfer(buffer_description);
    }
    channel.set_minor_loop_elements::<E>(1);
    channel.set_transfer_iterations(buffer.len() as u16);
    destination.enable_destination();
    let on_drop = |channel: &mut Channel| {
        destination.disable_destination();
        while channel.is_hardware_signaling() {}
    };
    unsafe { interrupt::interrupt(channel, on_drop) }.await?;
    Ok(buffer.len())
}

/// # Safety
///
/// Caller must ensure that `data` outlives, and remains valid for, the lifetime
/// of the DMA operation.
pub async unsafe fn transfer_raw<D, E>(
    channel: &mut Channel,
    destination: &D,
    data: *const E,
    len: usize,
) -> Result<usize>
where
    D: Destination<E>,
    E: Element,
{
    transfer(channel, destination, core::slice::from_raw_parts(data, len)).await
}
