# gapless-lossy-codec
Proof of concept lossy audio codec written in Rust that preserves gapless playback.

## Development Status
Please note that this is currently a very buggy implementation. It does technically achieve gapless playback, but at the cost of doubling the input filesize and mangling the sample rate. Why this happens is still under investigation.
