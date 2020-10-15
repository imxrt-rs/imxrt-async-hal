INCLUDE device.x /* TODO conditionally emit based on 'rt' flag */

MEMORY
{
    RAM     (rwx): ORIGIN = 0x20200000, LENGTH = 512K
    FLASH   (rwx): ORIGIN = 0x60000000, LENGTH = 1984K
}

/* Symbol provided by Rust */
EXTERN(Reset);
/* This might get stripped out in dependent crates, but it's important to keep around. */
/* It's put into the FCB block below. */
EXTERN(FLEXSPI_CONFIGURATION_BLOCK);

EXTERN(__EXCEPTIONS);
EXTERN(__INTERRUPTS);

EXTERN(DefaultHandler);
PROVIDE(NonMaskableInt = DefaultHandler);
EXTERN(HardFaultTrampoline);
PROVIDE(MemoryManagement = DefaultHandler);
PROVIDE(BusFault = DefaultHandler);
PROVIDE(UsageFault = DefaultHandler);
PROVIDE(SecureFault = DefaultHandler);
PROVIDE(SVCall = DefaultHandler);
PROVIDE(DebugMonitor = DefaultHandler);
PROVIDE(PendSV = DefaultHandler);
PROVIDE(SysTick = DefaultHandler);
PROVIDE(HardFault = HardFault_);
PROVIDE(DefaultHandler = DefaultHandler_);

PROVIDE(__pre_init = DefaultPreInit);

ENTRY(_ivt);

SECTIONS
{
    /* If you add more sections to FLASH, you must add this section here */
    __lflash = SIZEOF(.boot) + SIZEOF(.vector_table) + SIZEOF(.text) + SIZEOF(.rodata) + SIZEOF(.data) + SIZEOF(.gnu.sgstubs);

    /* The boot section contains all the special things that allow the IMXRT1062 to boot */
    .boot ORIGIN(FLASH) :
    {
        /* Firmware Configuration Block (FCB) */
        KEEP(*(.fcb));
        FILL(0xFFFFFFFF);
        . = ORIGIN(FLASH) + 0x1000;
        _ivt = .;
        /* ------------------
         * Image Vector Table
         * ------------------
         */
        LONG(0x402000D1);           /* Header, magic number */
        LONG(ADDR(.vector_table));  /* Address of the vectors table */
        LONG(0x00000000);           /* RESERVED */
        LONG(0x00000000);           /* Device Configuration Data (unused) */
        LONG(_boot_data);           /* Address to boot data */
        LONG(_ivt);                 /* Self reference, required by boot ROM */
        LONG(0x00000000);           /* Command Sequence File (unused) */
        LONG(0x00000000);           /* RESERVED */
        /* ---------
         * Boot data
         * ---------
         */
        _boot_data = .;
        LONG(ORIGIN(FLASH));        /* Start of image (origin of flash) */
        LONG(__lflash);             /* Length of flash */
        LONG(0x00000000);           /* Plugin flag (unused) */
        /* --------- */
    } > FLASH

    PROVIDE(_stack_start = ORIGIN(RAM) + LENGTH(RAM));

    /* ## Sections in FLASH */
    /* ### Vector table */
    .vector_table : ALIGN(1024)
    {
        __svectors = .;
        /* Initial Stack Pointer (SP) value */
        LONG(_stack_start);

        /* Reset vector */
        KEEP(*(.vector_table.reset_vector)); /* this is the `__RESET_VECTOR` symbol */
        __reset_vector = .;

        /* Exceptions */
        KEEP(*(.vector_table.exceptions)); /* this is the `__EXCEPTIONS` symbol */
        __eexceptions = .;

        /* Device specific interrupts */
        KEEP(*(.vector_table.interrupts)); /* this is the `__INTERRUPTS` symbol */
    } > FLASH

    PROVIDE(_stext = ADDR(.vector_table) + SIZEOF(.vector_table));

    /* ### .text */
    .text _stext :
    {
        /* place these 2 close to each other or the `b` instruction will fail to link */
        *(.PreResetTrampoline);
        *(.Reset);

        *(.text .text.*);
        *(.HardFaultTrampoline);
        *(.HardFault.*);
        . = ALIGN(4); /* Pad .text to the alignment to workaround overlapping load section bug in old lld */
    } > FLASH
    . = ALIGN(4); /* Ensure __etext is aligned if something unaligned is inserted after .text */
    __etext = .; /* Define outside of .text to allow using INSERT AFTER .text */

    /* ### .rodata */
    .rodata __etext : ALIGN(4)
    {
        *(.rodata .rodata.*);

        /* 4-byte align the end (VMA) of this section.
        This is required by LLD to ensure the LMA of the following .data
        section will have the correct alignment. */
        . = ALIGN(4);
    } > FLASH
    . = ALIGN(4); /* Ensure __erodata is aligned if something unaligned is inserted after .rodata */
    __erodata = .;

    /* ### .gnu.sgstubs
        This section contains the TrustZone-M veneers put there by the Arm GNU linker. */
    . = ALIGN(32); /* Security Attribution Unit blocks must be 32 bytes aligned. */
    __veneer_base = ALIGN(4);
    .gnu.sgstubs : ALIGN(4)
    {
        *(.gnu.sgstubs*)
        . = ALIGN(4); /* 4-byte align the end (VMA) of this section */
    } > FLASH
    . = ALIGN(4); /* Ensure __veneer_limit is aligned if something unaligned is inserted after .gnu.sgstubs */
    __veneer_limit = .;

    /* ## Sections in RAM */
    /* ### .data */
    .data : ALIGN(4)
    {
        . = ALIGN(4);
        __sdata = .;
        *(.data .data.*);
        . = ALIGN(4); /* 4-byte align the end (VMA) of this section */
    } > RAM AT>FLASH
    . = ALIGN(4); /* Ensure __edata is aligned if something unaligned is inserted after .data */
    __edata = .;

    /* LMA of .data */
    __sidata = LOADADDR(.data);

    /* ### .bss */
    . = ALIGN(4);
    __sbss = .; /* Define outside of section to include INSERT BEFORE/AFTER symbols */
    .bss (NOLOAD) : ALIGN(4)
    {
        *(.bss .bss.*);
        *(COMMON); /* Uninitialized C statics */
        . = ALIGN(4); /* 4-byte align the end (VMA) of this section */
    } > RAM
    . = ALIGN(4); /* Ensure __ebss is aligned if something unaligned is inserted after .bss */
    __ebss = .;

    /* ### .uninit */
    .uninit (NOLOAD) : ALIGN(4)
    {
        . = ALIGN(4);
        *(.uninit .uninit.*);
        . = ALIGN(4);
    } > RAM

    /* Place the heap right after `.uninit` */
    . = ALIGN(4);
    __sheap = .;

    /* ## .got */
    /* Dynamic relocations are unsupported. This section is only used to detect relocatable code in
        the input files and raise an error if relocatable code is found */
    .got (NOLOAD) :
    {
        KEEP(*(.got .got.*));
    }

    /* ## Discarded sections */
    /DISCARD/ :
    {
        /* Unused exception related info that only wastes space */
        *(.ARM.exidx);
        *(.ARM.exidx.*);
        *(.ARM.extab.*);
    }
}

/* Do not exceed this mark in the error messages below                                    | */
/* # Alignment checks */
ASSERT(ORIGIN(FLASH) % 4 == 0, "
ERROR(cortex-m-rt): the start of the FLASH region must be 4-byte aligned");

ASSERT(ORIGIN(RAM) % 4 == 0, "
ERROR(cortex-m-rt): the start of the RAM region must be 4-byte aligned");

ASSERT(__sdata % 4 == 0 && __edata % 4 == 0, "
BUG(cortex-m-rt): .data is not 4-byte aligned");

ASSERT(__sidata % 4 == 0, "
BUG(cortex-m-rt): the LMA of .data is not 4-byte aligned");

ASSERT(__sbss % 4 == 0 && __ebss % 4 == 0, "
BUG(cortex-m-rt): .bss is not 4-byte aligned");

ASSERT(__sheap % 4 == 0, "
BUG(cortex-m-rt): start of .heap is not 4-byte aligned");

/* # Position checks */

/* ## .vector_table */
ASSERT(__reset_vector == ADDR(.vector_table) + 0x8, "
BUG(cortex-m-rt): the reset vector is missing");

ASSERT(__eexceptions == ADDR(.vector_table) + 0x40, "
BUG(cortex-m-rt): the exception vectors are missing");

ASSERT(SIZEOF(.vector_table) > 0x40, "
ERROR(cortex-m-rt): The interrupt vectors are missing.
Possible solutions, from most likely to less likely:
- Link to a svd2rust generated device crate
- Check that you actually use the device/hal/bsp crate in your code
- Disable the 'device' feature of cortex-m-rt to build a generic application (a dependency
may be enabling it)
- Supply the interrupt handlers yourself. Check the documentation for details.");

/* ## .text */
ASSERT(ADDR(.vector_table) + SIZEOF(.vector_table) <= _stext, "
ERROR(cortex-m-rt): The .text section can't be placed inside the .vector_table section
Set _stext to an address greater than the end of .vector_table (See output of `nm`)");

ASSERT(_stext + SIZEOF(.text) < ORIGIN(FLASH) + LENGTH(FLASH), "
ERROR(cortex-m-rt): The .text section must be placed inside the FLASH memory.
Set _stext to an address smaller than 'ORIGIN(FLASH) + LENGTH(FLASH)'");

/* # Other checks */
ASSERT(SIZEOF(.got) == 0, "
ERROR(cortex-m-rt): .got section detected in the input object files
Dynamic relocations are not supported. If you are linking to C code compiled using
the 'cc' crate then modify your build script to compile the C code _without_
the -fPIC flag. See the documentation of the `cc::Build.pic` method for details.");
/* Do not exceed this mark in the error messages above                                    | */
