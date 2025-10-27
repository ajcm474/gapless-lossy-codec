# gapless-lossy-codec
Proof of concept lossy audio codec written in Rust that preserves gapless playback.

## Development Status
Please note that this is currently a somewhat buggy implementation. 
It does achieve gapless playback and lossy compression, 
but at the cost of mangling the amplitude as much as 25% in a few outlier samples. 
Why this happens is still under investigation.

# GLC Command-Line Interface

## Overview

The `glc` (Gapless Lossy Codec) binary now supports both GUI and command-line operation:

- **No arguments**: Opens the graphical user interface (when built with UI feature)
- **With arguments**: Encodes audio files to GLC format

## Command-Line Usage

### Basic Usage

```bash
glc <file1.wav> [file2.wav] [file3.flac] ...
```

### Supported Formats

- WAV files (`.wav`)
- FLAC files (`.flac`)

### Behavior

1. Each input file is encoded to a `.glc` file with the same base name
2. Example: `song.wav` → `song.glc`
3. Multiple files can be processed in one command
4. If any file fails, the program continues with remaining files but exits with code 1

### Examples

#### Encode a single file
```bash
glc audio.wav
# Creates audio.glc
```

#### Encode multiple files
```bash
glc song1.wav song2.wav song3.flac
# Creates song1.glc, song2.glc, song3.glc
```

#### Error handling
```bash
glc missing.wav  # Error: File not found
glc song.mp3     # Error: Unsupported file type
```

## Output

For each successfully encoded file, the program displays:
- Input filename
- Sample rate, channel count, and sample count
- Output filename, size, and compression ratio

Example output:
```
Loading: "test.wav"
Encoding: 44100 Hz, 2 channels, 88200 samples
Saved: "test.glc" (7014 bytes, 4.0% of original)
```

## Build Options

### With GUI support (default)
```bash
cargo build --release
```
Requires system libraries: glib-2.0, libFLAC

### CLI-only (no GUI)
```bash
cargo build --release --no-default-features
```
Produces a smaller binary without GUI dependencies.

## Implementation Details

The CLI functionality was added to `src/main.rs`:
- `encode_file()`: Encodes a single audio file
- `is_audio_file()`: Validates file extensions
- `main()`: Routes to CLI or GUI based on arguments

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