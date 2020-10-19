//! Supporting traits for defining peripheral DMA
//! sources and destinations

use super::{interrupt, Channel, Destination, Element, Error, Source};

pub type Result<T> = core::result::Result<T, Error>;

pub async fn receive<S, E>(channel: &mut Channel, source: &S, buffer: &mut [E]) -> Result<usize>
where
    S: Source<E>,
    E: Element,
{
    channel.set_trigger_from_hardware(Some(source.source_signal()));
    unsafe {
        channel.set_source_transfer(&super::Transfer::hardware(source.source()));
    }
    unsafe {
        let buffer_description = super::Transfer::buffer_linear(buffer.as_ptr(), buffer.len());
        channel.set_destination_transfer(&buffer_description);
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
        channel.set_destination_transfer(&super::Transfer::hardware(destination.destination()));

        let buffer_description = super::Transfer::buffer_linear(buffer.as_ptr(), buffer.len());
        channel.set_source_transfer(&buffer_description);
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
