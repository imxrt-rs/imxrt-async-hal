//! Direct Memory Access (DMA) for async I/O
//!
//! DMA [`Channel`s](struct.Channel.html) power some asynchronous I/O operations.
//! Use [`channels`](fn.channels.html) to acquire all of the processor's DMA channels.
//! Then, use the `Channel`s in APIs that require them. The implementation handles
//! DMA receive and transfer operations, and ensures that the lifetime of your buffers
//! is correct.
//!
//! The implementation also provides a [`pipe`](pipe/index.html), a hardware-backed
//! communication channels (not to be confused with DMA `Channel`s). Use `pipe` senders
//! and receivers to synchronize tasks, and transmit `Copy` data between tasks using
//! DMA hardware.

#![allow(non_snake_case)] // Compatibility with RAL

mod chip;
mod element;
mod interrupt;
mod peripheral;
#[cfg(feature = "pipe")]
#[cfg_attr(docsrs, doc(cfg(feature = "pipe")))]
pub mod pipe;
mod register;

use element::Element;
pub(crate) use peripheral::{receive, receive_raw, transfer, transfer_raw, Destination, Source};

use crate::ral;
pub use chip::CHANNEL_COUNT;
use core::{
    fmt::{self, Debug},
    mem,
};
pub use register::tcd::BandwidthControl;
use register::{DMARegisters, MultiplexerRegisters, Static, DMA, MULTIPLEXER};

/// A DMA channel
///
/// DMA channels provide one-way transfers of data. They're used in peripheral APIs,
/// as well as [`pipe`](../pipe/index.html) channels.
///
/// Use [`channels`](fn.channels.html) to acquire all of the DMA channels.
pub struct Channel {
    /// Our channel number, expected to be between 0 to (CHANNEL_COUNT - 1)
    index: usize,
    /// Reference to the DMA registers
    registers: Static<DMARegisters>,
    /// Reference to the DMA multiplexer
    multiplexer: Static<MultiplexerRegisters>,
}

impl Channel {
    /// Set the channel's bandwidth control
    ///
    /// - `None` disables bandwidth control (default setting)
    /// - `Some(bwc)` sets the bandwidth control to `bwc`
    pub fn set_bandwidth_control(&mut self, bandwidth: Option<BandwidthControl>) {
        let raw = BandwidthControl::raw(bandwidth);
        let tcd = self.tcd();
        ral::modify_reg!(register::tcd, tcd, CSR, BWC: raw);
    }

    /// Returns the DMA channel number
    ///
    /// Channels are unique and numbered within the half-open range `[0, CHANNEL_COUNT)`.
    pub fn channel(&self) -> usize {
        self.index
    }

    /// Creates a DMA channel
    ///
    /// # Safety
    ///
    /// A `Channel` will be temporarily allocated in an interrupt to disable it.
    /// This `Channel` will alias the other `Channel` that's in use by the user.
    #[inline(always)]
    const unsafe fn new(index: usize) -> Self {
        Channel {
            index,
            registers: DMA,
            multiplexer: MULTIPLEXER,
        }
    }

    /// Returns a handle to this channel's transfer control descriptor
    fn tcd(&self) -> &register::TransferControlDescriptor {
        &self.registers.TCD[self.index]
    }

    /// Prepare the source of a transfer; see [`Transfer`](struct.Transfer.html) for details.
    ///
    /// # Safety
    ///
    /// Address pointer must be valid for lifetime of the transfer.
    unsafe fn set_source_transfer<T: Into<Transfer<E>>, E: Element>(&mut self, transfer: T) {
        let tcd = self.tcd();
        let transfer = transfer.into();
        ral::write_reg!(register::tcd, tcd, SADDR, transfer.address as u32);
        ral::write_reg!(register::tcd, tcd, SOFF, transfer.offset);
        ral::modify_reg!(register::tcd, tcd, ATTR, SSIZE: E::DATA_TRANSFER_ID, SMOD: transfer.modulo);
        ral::write_reg!(register::tcd, tcd, SLAST, transfer.last_address_adjustment);
    }

    /// Prepare the destination for a transfer; see [`Transfer`](struct.Transfer.html) for details.
    ///
    /// # Safety
    ///
    /// Address pointer must be valid for lifetime of the transfer.
    unsafe fn set_destination_transfer<T: Into<Transfer<E>>, E: Element>(&mut self, transfer: T) {
        let tcd = self.tcd();
        let transfer = transfer.into();
        ral::write_reg!(register::tcd, tcd, DADDR, transfer.address as u32);
        ral::write_reg!(register::tcd, tcd, DOFF, transfer.offset);
        ral::modify_reg!(register::tcd, tcd, ATTR, DSIZE: E::DATA_TRANSFER_ID, DMOD: transfer.modulo);
        ral::write_reg!(
            register::tcd,
            tcd,
            DLAST_SGA,
            transfer.last_address_adjustment
        );
    }

    /// Set the number of *bytes* to transfer per minor loop
    ///
    /// Describes how many bytes we should transfer for each DMA service request.
    fn set_minor_loop_bytes(&mut self, nbytes: u32) {
        let tcd = self.tcd();
        ral::write_reg!(register::tcd, tcd, NBYTES, nbytes);
    }

    /// Se the number of elements to move in each minor loop
    ///
    /// Describes how many elements we should transfer for each DMA service request.
    fn set_minor_loop_elements<E: Element>(&mut self, len: usize) {
        self.set_minor_loop_bytes((mem::size_of::<E>() * len) as u32);
    }

    /// Tells the DMA channel how many transfer iterations to perform
    ///
    /// A 'transfer iteration' is a read from a source, and a write to a destination, with
    /// read and write sizes described by a minor loop. Each iteration requires a DMA
    /// service request, either from hardware or from software.
    fn set_transfer_iterations(&mut self, iterations: u16) {
        let tcd = self.tcd();
        ral::write_reg!(register::tcd, tcd, CITER, iterations);
        ral::write_reg!(register::tcd, tcd, BITER, iterations);
    }

    /// Enable or disabling triggering from hardware
    ///
    /// If source is `Some(value)`, we trigger from hardware identified by the source identifier.
    /// If `source` is `None`, we disable hardware triggering.
    fn set_trigger_from_hardware(&mut self, source: Option<u32>) {
        let chcfg = &self.multiplexer.chcfg[self.index];
        chcfg.write(0);
        if let Some(source) = source {
            chcfg.write(MultiplexerRegisters::ENBL | source);
        }
    }

    /// Set this DMA channel as always on
    ///
    /// Use `set_always_on()` so that the DMA multiplexer drives the transfer with no
    /// throttling. Specifically, an "always-on" transfer will not need explicit re-activiation
    /// between major loops.
    ///
    /// Use an always-on channel for memory-to-memory transfers, so that you don't need explicit
    /// software re-activation to maintain the transfer. On the other hand, most peripheral transfers
    /// should not use an always-on channel, since the peripheral should control the data flow through
    /// explicit activation.
    fn set_always_on(&mut self) {
        let chcfg = &self.multiplexer.chcfg[self.index];
        chcfg.write(0);
        chcfg.write(MultiplexerRegisters::ENBL | MultiplexerRegisters::A_ON);
    }

    /// Returns `true` if the DMA channel is receiving a service signal from hardware
    fn is_hardware_signaling(&self) -> bool {
        self.registers.HRS.read() & (1 << self.index) != 0
    }

    /// Enable or disable the DMA's multiplexer request
    ///
    /// In this DMA implementation, all peripheral transfers and memcpy requests
    /// go through the DMA multiplexer. So, this needs to be set for the multiplexer
    /// to service the channel.
    fn set_enable(&mut self, enable: bool) {
        if enable {
            self.registers.SERQ.write(self.index as u8);
        } else {
            self.registers.CERQ.write(self.index as u8);
        }
    }

    /// Returns `true` if this DMA channel generated an interrupt
    fn is_interrupt(&self) -> bool {
        self.registers.INT.read() & (1 << self.index) != 0
    }

    /// Clear the interrupt flag from this DMA channel
    fn clear_interrupt(&mut self) {
        self.registers.CINT.write(self.index as u8);
    }

    /// Enable or disable 'disable on completion'
    ///
    /// 'Disable on completion' lets the DMA channel automatically clear the request signal
    /// when it completes a transfer.
    fn set_disable_on_completion(&mut self, dreq: bool) {
        let tcd = self.tcd();
        ral::modify_reg!(register::tcd, tcd, CSR, DREQ: dreq as u16);
    }

    /// Enable or disable interrupt generation when the transfer completes
    ///
    /// You're responsible for registering your interrupt handler.
    fn set_interrupt_on_completion(&mut self, intr: bool) {
        let tcd = self.tcd();
        ral::modify_reg!(register::tcd, tcd, CSR, INTMAJOR: intr as u16);
    }

    /// Indicates if the DMA transfer has completed
    fn is_complete(&self) -> bool {
        let tcd = self.tcd();
        ral::read_reg!(register::tcd, tcd, CSR, DONE == 1)
    }

    /// Clears completion indication
    fn clear_complete(&mut self) {
        self.registers.CDNE.write(self.index as u8);
    }

    /// Indicates if the DMA channel is in an error state
    fn is_error(&self) -> bool {
        self.registers.ERR.read() & (1 << self.index) != 0
    }

    /// Clears the error flag
    fn clear_error(&mut self) {
        self.registers.CERR.write(self.index as u8);
    }

    /// Indicates if this DMA channel is enabled
    fn is_enabled(&self) -> bool {
        self.registers.ERQ.read() & (1 << self.index) != 0
    }

    /// Returns the value from the **global** error status register
    ///
    /// It may reflect the last channel that produced an error, and that
    /// may not be related to this channel.
    fn error_status(&self) -> u32 {
        self.registers.ES.read()
    }

    /// Start a DMA transfer
    ///
    /// `start()` should be used to request service from the DMA controller. It's
    /// necessary for in-memory DMA transfers. Do not use it for hardware-initiated
    /// DMA transfers. DMA transfers that involve hardware will rely on the hardware
    /// to request DMA service.
    ///
    /// Flag is automatically cleared by hardware after it's asserted.
    fn start(&mut self) {
        self.registers.SSRT.write(self.index as u8);
    }

    /// Get and change the state that's shared between this channel and the ISR
    ///
    /// # Safety
    ///
    /// This should only be used when interrupts are disabled, and there are no active
    /// transfers.
    unsafe fn shared_mut(
        &mut self,
    ) -> &'static mut [interrupt::Shared; interrupt::NUM_SHARED_STATES] {
        &mut interrupt::SHARED_STATES[self.index]
    }

    /// Get the state that's shared between this channel and the ISR
    ///
    /// # Safety
    ///
    /// This should only be used when interrupts are disabled, and there are no active
    /// transfers.
    unsafe fn shared(&self) -> &'static [interrupt::Shared; interrupt::NUM_SHARED_STATES] {
        &interrupt::SHARED_STATES[self.index]
    }
}

/// An error when preparing a transfer
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// There is already a scheduled transfer
    ///
    /// Cancel the transfer and try again.
    ScheduledTransfer,
    /// Error setting up the DMA transfer
    Setup(ErrorStatus),
    /// The operation was cancelled
    ///
    /// `Cancelled` is the return from a [`pipe`](pipe/index.html) sender or
    /// receiver when the other half is dropped.
    Cancelled,
}

/// Initialize and acquire the DMA channels
///
/// The return is 32 channels. However, **only the first [`CHANNEL_COUNT`](constant.CHANNEL_COUNT.html) channels
/// are initialized to `Some(channel)`. The rest are `None`**.
///
/// You should enable the clock gates before calling `channels`. See
/// [`ccm::Handle::clock_gate_dma`](../ccm/struct.Handle.html#method.clock_gate_dma) for more information.
///
/// # Example
///
/// Initialize and acquire the DMA channels, and move channel 7 to another function:
///
/// ```no_run
/// use imxrt_async_hal as hal;
/// use hal::{ccm::{CCM, ClockGate}, dma};
/// use hal::ral::{dma0, dmamux, ccm};
///
/// fn prepare_peripheral(channel: dma::Channel) { /* ... */ }
///
/// let mut ccm = ccm::CCM::take().map(CCM::new).unwrap();
/// let mut dma = dma0::DMA0::take().unwrap();
/// ccm.handle.clock_gate_dma(&mut dma, ClockGate::On);
/// let mut channels = dma::channels(
///     dma,
///     dmamux::DMAMUX::take().unwrap(),
/// );
///
/// prepare_peripheral(channels[7].take().unwrap());
/// ```
pub fn channels(dma: ral::dma0::Instance, mux: ral::dmamux::Instance) -> [Option<Channel>; 32] {
    drop(dma);
    drop(mux);

    let mut channels = chip::DMA_CHANNEL_INIT;

    for (idx, channel) in channels.iter_mut().take(CHANNEL_COUNT).enumerate() {
        let c = unsafe { Channel::new(idx) };
        c.tcd().reset();
        *channel = Some(c);
    }

    #[cfg(not(feature = "imxrt101x"))]
    unsafe {
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA0_DMA16);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA1_DMA17);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA2_DMA18);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA3_DMA19);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA4_DMA20);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA5_DMA21);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA6_DMA22);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA7_DMA23);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA8_DMA24);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA9_DMA25);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA10_DMA26);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA11_DMA27);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA12_DMA28);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA13_DMA29);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA14_DMA30);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA15_DMA31);
    };
    #[cfg(feature = "imxrt101x")]
    unsafe {
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA0);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA1);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA2);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA3);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA4);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA5);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA6);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA7);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA8);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA9);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA10);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA11);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA12);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA13);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA14);
        cortex_m::peripheral::NVIC::unmask(crate::ral::interrupt::DMA15);
    };

    channels
}

/// A wrapper around a DMA error status value
///
/// The wrapper contains a copy of the DMA controller's
/// error status register at the point of an error. The
/// wrapper implements both `Debug` and `Display`. The
/// type may be printed to understand why there was a
/// DMA error.
#[derive(Clone, Copy)]
pub struct ErrorStatus {
    /// The raw error status
    es: u32,
}

impl ErrorStatus {
    const fn new(es: u32) -> Self {
        ErrorStatus { es }
    }
}

impl Debug for ErrorStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DMA_ES({:#010X})", self.es)
    }
}

/// Describes a DMA transfer
///
/// `Transfer` describes a source or a destination of a DMA transfer. See the member
/// documentation for details.
#[derive(Clone, Copy, Debug)]
struct Transfer<E: Element> {
    /// The starting address for the DMA transfer
    ///
    /// If this describes a source, `address` will be the first
    /// address read. If this describes a destination, `address`
    /// will be the first address written.
    address: *const E,

    /// Offsets to perform for each read / write of a memory address.
    ///
    /// When defining a transfer for a peripheral source or destination,
    /// `offset` should be zero. Otherwise, `offset` should represent the
    /// size of the data element, `E`.
    ///
    /// Negative (backwards) adjustments are permitted, if you'd like to read
    /// a buffer backwards or something.
    offset: i16,

    /* size: u16, // Not needed; captured in E: Element type */
    /// Defines the strategy for reading / writing linear or circular buffers
    ///
    /// `modulo` should be zero if this definition defines a transfer from linear
    /// memory or a peripheral. `modulo` will be non-zero when defining a transfer
    /// from a circular buffer. The non-zero value is the number of high bits to freeze
    /// when performing address offsets (see `offset`). Given that we're only supporting
    /// power-of-two buffer sizes, `modulo` will be `31 - clz(cap * sizeof(E))`, where `cap` is the
    /// total size of the circular buffer, `clz` is "count leading zeros," and `sizeof(E)` is
    /// the size of the element, in bytes.
    modulo: u16,

    /// Perform any last-address adjustments when we complete the transfer
    ///
    /// Once we complete moving data from a linear buffer, we should set our pointer back to the
    /// initial address. For this case, `last_address_adjustment` should be a negative number that
    /// describes how may *bytes* to move backwards from our current address to reach our starting
    /// address. Alternatively, it could describe how to move to a completely new address, like
    /// a nearby buffer that we're using for a double-buffer. Or, set it to zero, which means "keep
    /// your current position." "Keep your current position" is important when working with a
    /// peripheral address!
    last_address_adjustment: i32,
}

impl<E: Element> Transfer<E> {
    fn hardware(address: *const E) -> Self {
        Transfer {
            address,
            // Don't move the address pointer
            offset: 0,
            // We're not a circular buffer
            modulo: 0,
            // Don't move the address pointer
            last_address_adjustment: 0,
        }
    }

    fn buffer(buffer: &[E]) -> Self {
        Transfer {
            address: buffer.as_ptr(),
            offset: core::mem::size_of::<E>() as i16,
            modulo: 0,
            last_address_adjustment: ((buffer.len() * mem::size_of::<E>()) as i32).wrapping_neg(),
        }
    }

    fn buffer_mut(buffer: &mut [E]) -> Self {
        Transfer {
            address: buffer.as_ptr(),
            offset: core::mem::size_of::<E>() as i16,
            modulo: 0,
            last_address_adjustment: ((buffer.len() * mem::size_of::<E>()) as i32).wrapping_neg(),
        }
    }
}

// It's OK to send a channel across an execution context.
// They can't be cloned or copied, so there's no chance of
// them being (mutably) shared.
unsafe impl Send for Channel {}
