## Version 0.4.0
- Breaking change: `Encoder::new` now requires that a sample rate be specified
  - This ensures that the pre-computed weights use the correct nyquist value
  - The difference isn't huge between 44100 and 48000, but for non-standard values the calculation would be way off
  - I can't think of any reasonable situation where it makes sense to reuse the same Encoder instance with different sample rates
- Breaking change: `Encoder::encode` no longer takes the sample rate as a parameter, since it's already stored in the `Encoder` object
- Switch to variable quantization ranging from 8-bit to 16-bit depending on sample importance
- Add more docstrings

## Version 0.3.0
- Add proper psychoacoustic masking to achieve lossy compression with significant filesize reduction
- Switch to FLAC export instead of raw PCM to avoid sample rate confusion in external programs
- Keep track of sample rate in the decoder so that UI playback happens at the right speed
- Add fallback option to export to WAV if `libFLAC` is unavailable or `export-flac` feature is disabled
- Add `build.rs` file to help Cargo find the `libFLAC` executable
- Add performance tests

## Version 0.2.0
- Use pre-computed tables for MDCT and parallelize the heavy computation
- Fix amplitude normalization
- Add unit tests

## Version 0.1.1
- Switch from 8-bit to 16-bit quantization
- Properly handle stereo as interleaved samples
- Use FFT-based MDCT instead of simplified DCT