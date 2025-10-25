# gapless-lossy-codec
Proof of concept lossy audio codec written in Rust that preserves gapless playback.

## Development Status
Please note that this is currently a very buggy implementation. It does technically achieve gapless playback, but at the cost of doubling the input filesize and mangling the sample rate. Why this happens is still under investigation.

## Export Functionality
The codec supports exporting decoded audio to formats with embedded metadata (sample rate, channels, etc.):
- **FLAC format** (default): Enabled by default when system libflac is installed. Uses `export_to_flac()`.
- **WAV format** (fallback): Used when libflac is not available or `flac-export` feature is disabled. Uses `export_to_wav()`.

This solves the issue of forgetting sample rates when working with raw PCM exports, as both FLAC and WAV include metadata in their file headers.

### Building without FLAC
If libflac is not installed on your system, you can build without FLAC support:
```bash
# For library only
cargo build --lib --no-default-features

# For GUI application (keeps UI, disables FLAC)
cargo build --no-default-features --features ui
```

To install libflac on various systems:
```bash
# Debian/Ubuntu
sudo apt-get install libflac-dev

# Arch Linux
sudo pacman -S flac

# macOS
brew install flac
```