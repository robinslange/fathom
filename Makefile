.PHONY: build check test fmt lint clean cli desktop dev

build:
	cargo build --workspace --release

check:
	cargo check --workspace --all-targets

test:
	cargo test --workspace

fmt:
	cargo fmt --all

lint:
	cargo clippy --workspace --all-targets -- -D warnings

clean:
	cargo clean
	rm -rf apps/desktop/node_modules apps/desktop/dist

cli:
	cargo run -p fathom-cli --release -- $(ARGS)

desktop:
	cd apps/desktop && pnpm tauri dev

desktop-build:
	cd apps/desktop && pnpm tauri build
