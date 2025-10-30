# gapless-lossy-codec
Proof of concept lossy audio codec written in Rust that preserves gapless playback.

## Development Status
Please note that this is currently a somewhat buggy implementation. 
It does achieve gapless playback and lossy compression, 
but at the cost of mangling the amplitude as much as 25% in a few outlier samples. 
Why this happens is still under investigation.

## Command-Line Usage (Encoding)
Basic usage
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

## Command-Line Usage (Decoding)
Decode a file and save output (FLAC by default)
```bash
glc -d file.glc
```
Creates `file.flac` with default compression level 5

Decode with specific FLAC compression level (0-8)
```bash
glc -d --flac-level 8 file.glc
```

Decode to WAV format instead
```bash
glc -d --wav file.glc
```

Decode a file and play it back using a pure Rust implementation 
(requires `playback` or `ui` feature to be enabled):
```bash
glc -p file.glc
```

Decode a file and play it back using ffplay (may not work currently):
```bash
glc -p file.glc --ffplay
```

## Features Overview
No features are enabled by default for a minimal command-line build.
You can enable specific features as needed.

### Build with default settings (command-line only)
```bash
cargo build --release
```
Produces a minimal binary with encoding/decoding capabilities only.

### Build with GUI support
```bash
cargo build --release --features ui
```
Includes GUI support. May require system libraries for GUI (glib-2.0) and audio playback (alsa on Linux).

### Build with playback support (no GUI)
```bash
cargo build --release --features playback
```
Enables audio playback through the command line using rodio.

### FLAC Support
FLAC encoding and decoding is now implemented in pure Rust, requiring no external libraries.
The encoder supports compression levels 0-8, with level 5 as the default.
This implementation follows RFC 9639 and produces standard-compliant FLAC files.