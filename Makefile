PNPM ?= pnpm
CARGO ?= cargo
VENDOR_CORE := $(CURDIR)/vendor/sing-box-1.14.5-linux-amd64/sing-box

.PHONY: help install build frontend backend helper test smoke install-helper clean

help:
	@echo "Veil targets:"
	@echo "  install        pnpm install (frontend dependencies)"
	@echo "  build          pnpm build + cargo build --release"
	@echo "  frontend       pnpm build"
	@echo "  backend        cargo build --release (src-tauri)"
	@echo "  helper         cargo build --release (veil-helper)"
	@echo "  test           cargo test (backend) + helper tests"
	@echo "  smoke          real sing-box smoke test via examples/engine_smoke.rs"
	@echo "  install-helper build + sudo install of veil-helper and polkit policy"
	@echo "  clean          remove build outputs"

install:
	$(PNPM) install

frontend:
	$(PNPM) build

backend:
	cd src-tauri && $(CARGO) build --release

helper:
	$(CARGO) build --release --manifest-path src-tauri/helper/Cargo.toml

build: frontend backend helper

test:
	cd src-tauri && $(CARGO) test
	$(CARGO) test --manifest-path src-tauri/helper/Cargo.toml

smoke:
	cd src-tauri && VEIL_SINGBOX="$(VENDOR_CORE)" $(CARGO) run --example engine_smoke

install-helper:
	./install-helper.sh

clean:
	rm -rf dist
	cd src-tauri && $(CARGO) clean
	$(CARGO) clean --manifest-path src-tauri/helper/Cargo.toml
