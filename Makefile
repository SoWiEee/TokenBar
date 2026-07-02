# Build order matters: the Rust staticlib must exist before swift build links.
# Run everything from the repo root (the -L path in Package.swift is relative).

.PHONY: all rust build run clean linux linux-run deb

all: build

# --- Linux GTK frontend (crates/tokenbar-gtk) ---------------------------------
# Needs system GTK4 dev libs: libgtk-4-dev libadwaita-1-dev libepoxy-dev.
# On some NVIDIA/remote setups the app needs GDK_DEBUG=gl-glx to get a GL context.

linux:
	cargo build --release -p tokenbar-gtk

linux-run:
	GDK_DEBUG=gl-glx cargo run -p tokenbar-gtk

deb:
	cargo deb -p tokenbar-gtk

# --- GNOME Shell extension (top-bar button + popover) -------------------------
# Reads the app's cache; "Open TokenBar" launches the GTK app for the 3D graph.

EXT_UUID = tokenbar@nyanako.io
EXT_DEST = $(HOME)/.local/share/gnome-shell/extensions/$(EXT_UUID)

ext-install:
	mkdir -p "$(EXT_DEST)"
	cp -r gnome-extension/$(EXT_UUID)/. "$(EXT_DEST)/"
	@echo "Installed. Enable with:  gnome-extensions enable $(EXT_UUID)"
	@echo "Then reload the shell:   X11 → Alt+F2, type 'r', Enter   |   Wayland → log out/in"

rust:
	cargo build --release

build: rust
	@$(call relink_if_stale,debug)
	swift build

run: rust
	@$(call relink_if_stale,debug)
	swift run TokenBar

clean:
	cargo clean
	swift package clean

bundle: rust
	@$(call relink_if_stale,release)
	swift build -c release
	scripts/bundle.sh

# SwiftPM does not track the Rust staticlib as a dependency: with no Swift
# source changes it reuses the cached executable and silently ships stale
# Rust code. Drop the executable whenever the staticlib is newer.
define relink_if_stale
	if [ target/release/libtb_core_ffi.a -nt .build/$(1)/TokenBar ]; then \
		rm -f .build/$(1)/TokenBar; \
	fi
endef
