#!/bin/bash
# Run rax VM with proper terminal handling

# Save terminal settings
SAVED_STTY=$(stty -g)

# Set terminal to raw mode for character-by-character input
stty raw -echo

# Restore terminal on exit
cleanup() {
    stty "$SAVED_STTY"
    echo
    echo "VM exited."
}
trap cleanup EXIT

# Run the VM
cargo run --release -- \
    --kernel ./vmlinuz-6.17.0-8-generic \
    --initrd ./initrd.cpio.gz \
    --cmdline "console=ttyS0 rdinit=/init"
