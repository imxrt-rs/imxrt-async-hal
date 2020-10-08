CARGO ?= cargo
RUSTUP ?= rustup

THUMBV7EM_NONE_EABIHF_INSTALLED := $(shell $(RUSTUP) component list | grep 'rust-std-thumbv7em-none-eabihf.*(installed)' > /dev/null; echo $$?)
ifeq ($(THUMBV7EM_NONE_EABIHF_INSTALLED), 1)
  $(error Missing thumbv7em-none-eabihf target! Please run 'rustup target add thumbv7em-none-eabihf)
endif

CARGO_BINUTILS_INSTALLED := $(shell $(CARGO) install --list | grep 'cargo-binutils' >/dev/null; echo $$?)
ifeq ($(CARGO_BINUTILS_INSTALLED),1)
  $(error Missing cargo-binutils! Please run 'cargo install cargo-binutils')
endif

BINS ?= $(shell ls -1 src/bin | cut -f 1 -d .)
TARGET := target/thumbv7em-none-eabihf/release

.PHONY: all
all: build
	@for bin in $(BINS);\
		do rust-objcopy -O ihex $(TARGET)/$$bin $(TARGET)/$$bin.hex;\
		done

.PHONY: build
build:
	@$(CARGO) build --release --target thumbv7em-none-eabihf

.PHONY: clean
clean:
	@cargo clean

.PHONY: clippy
clippy:
	@cargo clippy --target thumbv7em-none-eabihf