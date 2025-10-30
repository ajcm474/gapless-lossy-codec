//! Pure Rust FLAC encoder implementation based on RFC 9639
//! Currently supports compression level 5 with 16-bit samples

use anyhow::{anyhow, Result};
use std::io::Write;
use std::path::Path;

/// FLAC file signature
const FLAC_SIGNATURE: [u8; 4] = [0x66, 0x4C, 0x61, 0x43]; // "fLaC"

/// Maximum Rice parameter value for 4-bit encoding
const MAX_RICE_PARAM_4BIT: u32 = 14;

/// Frame sync code
const FRAME_SYNC_CODE: u16 = 0x3FFE;

/// CRC-8 polynomial for FLAC (x^8 + x^2 + x^1 + x^0 = 0x07)
/// Build the lookup table at runtime to avoid the large constant
fn build_crc8_table() -> [u8; 256]
{
    let mut table = [0u8; 256];
    for i in 0..256
    {
        let mut crc = i as u8;
        for _ in 0..8
        {
            if crc & 0x80 != 0
            {
                crc = (crc << 1) ^ 0x07;
            }
            else
            {
                crc <<= 1;
            }
        }
        table[i] = crc;
    }
    table
}

/// Compute CRC-8 checksum using FLAC polynomial
fn crc8(data: &[u8]) -> u8
{
    let table = build_crc8_table();
    let mut crc = 0u8;
    for &byte in data
    {
        crc = table[(crc ^ byte) as usize];
    }
    crc
}

/// Compute CRC-16 checksum for FLAC (polynomial x^16 + x^15 + x^2 + x^0 = 0x8005)
fn crc16(data: &[u8]) -> u16
{
    let mut crc = 0u16;
    for &byte in data
    {
        crc = ((crc << 8) ^ crc16_table(((crc >> 8) ^ byte as u16) as u8)) & 0xFFFF;
    }
    crc
}

/// Generate CRC-16 table entry
fn crc16_table(val: u8) -> u16
{
    let mut crc = (val as u16) << 8;
    for _ in 0..8
    {
        if crc & 0x8000 != 0
        {
            crc = (crc << 1) ^ 0x8005;
        }
        else
        {
            crc <<= 1;
        }
    }
    crc & 0xFFFF
}

/// MD5 Context for computing audio checksum
struct MD5Context
{
    state: [u32; 4],
    count: [u32; 2],
    buffer: [u8; 64],
}

impl MD5Context
{
    fn new() -> Self
    {
        MD5Context
        {
            state: [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476],
            count: [0, 0],
            buffer: [0; 64],
        }
    }

    fn update(&mut self, data: &[u8])
    {
        let mut input_index = 0;
        let len = data.len();

        // Compute number of bytes mod 64
        let index = (self.count[0] >> 3) as usize & 0x3F;

        // Update number of bits
        self.count[0] = self.count[0].wrapping_add((len as u32) << 3);
        if self.count[0] < ((len as u32) << 3)
        {
            self.count[1] = self.count[1].wrapping_add(1);
        }
        self.count[1] = self.count[1].wrapping_add((len as u32) >> 29);

        let part_len = 64 - index;

        // Transform as many times as possible
        let mut i = if len >= part_len
        {
            self.buffer[index..index + part_len].copy_from_slice(&data[0..part_len]);
            self.transform(&self.buffer.clone());

            let mut i = part_len;
            while i + 63 < len
            {
                let mut chunk = [0u8; 64];
                chunk.copy_from_slice(&data[i..i + 64]);
                self.transform(&chunk);
                i += 64;
            }
            input_index = i;
            0
        }
        else
        {
            0
        };

        // Buffer remaining input
        if input_index < len
        {
            let remaining = len - input_index;
            let buffer_index = if input_index == 0 { index } else { 0 };
            self.buffer[buffer_index..buffer_index + remaining]
                .copy_from_slice(&data[input_index..]);
        }
    }

    fn transform(&mut self, block: &[u8; 64])
    {
        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];

        let mut x = [0u32; 16];
        for i in 0..16
        {
            x[i] = u32::from_le_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }

        // Round 1
        macro_rules! ff {
            ($a:expr, $b:expr, $c:expr, $d:expr, $x:expr, $s:expr, $ac:expr) => {
                $a = $a.wrapping_add((($b & $c) | (!$b & $d)).wrapping_add($x).wrapping_add($ac));
                $a = $a.rotate_left($s).wrapping_add($b);
            };
        }

        ff!(a, b, c, d, x[0], 7, 0xD76AA478);
        ff!(d, a, b, c, x[1], 12, 0xE8C7B756);
        ff!(c, d, a, b, x[2], 17, 0x242070DB);
        ff!(b, c, d, a, x[3], 22, 0xC1BDCEEE);
        ff!(a, b, c, d, x[4], 7, 0xF57C0FAF);
        ff!(d, a, b, c, x[5], 12, 0x4787C62A);
        ff!(c, d, a, b, x[6], 17, 0xA8304613);
        ff!(b, c, d, a, x[7], 22, 0xFD469501);
        ff!(a, b, c, d, x[8], 7, 0x698098D8);
        ff!(d, a, b, c, x[9], 12, 0x8B44F7AF);
        ff!(c, d, a, b, x[10], 17, 0xFFFF5BB1);
        ff!(b, c, d, a, x[11], 22, 0x895CD7BE);
        ff!(a, b, c, d, x[12], 7, 0x6B901122);
        ff!(d, a, b, c, x[13], 12, 0xFD987193);
        ff!(c, d, a, b, x[14], 17, 0xA679438E);
        ff!(b, c, d, a, x[15], 22, 0x49B40821);

        // Round 2
        macro_rules! gg {
            ($a:expr, $b:expr, $c:expr, $d:expr, $x:expr, $s:expr, $ac:expr) => {
                $a = $a.wrapping_add((($b & $d) | ($c & !$d)).wrapping_add($x).wrapping_add($ac));
                $a = $a.rotate_left($s).wrapping_add($b);
            };
        }

        gg!(a, b, c, d, x[1], 5, 0xF61E2562);
        gg!(d, a, b, c, x[6], 9, 0xC040B340);
        gg!(c, d, a, b, x[11], 14, 0x265E5A51);
        gg!(b, c, d, a, x[0], 20, 0xE9B6C7AA);
        gg!(a, b, c, d, x[5], 5, 0xD62F105D);
        gg!(d, a, b, c, x[10], 9, 0x02441453);
        gg!(c, d, a, b, x[15], 14, 0xD8A1E681);
        gg!(b, c, d, a, x[4], 20, 0xE7D3FBC8);
        gg!(a, b, c, d, x[9], 5, 0x21E1CDE6);
        gg!(d, a, b, c, x[14], 9, 0xC33707D6);
        gg!(c, d, a, b, x[3], 14, 0xF4D50D87);
        gg!(b, c, d, a, x[8], 20, 0x455A14ED);
        gg!(a, b, c, d, x[13], 5, 0xA9E3E905);
        gg!(d, a, b, c, x[2], 9, 0xFCEFA3F8);
        gg!(c, d, a, b, x[7], 14, 0x676F02D9);
        gg!(b, c, d, a, x[12], 20, 0x8D2A4C8A);

        // Round 3
        macro_rules! hh {
            ($a:expr, $b:expr, $c:expr, $d:expr, $x:expr, $s:expr, $ac:expr) => {
                $a = $a.wrapping_add(($b ^ $c ^ $d).wrapping_add($x).wrapping_add($ac));
                $a = $a.rotate_left($s).wrapping_add($b);
            };
        }

        hh!(a, b, c, d, x[5], 4, 0xFFFA3942);
        hh!(d, a, b, c, x[8], 11, 0x8771F681);
        hh!(c, d, a, b, x[11], 16, 0x6D9D6122);
        hh!(b, c, d, a, x[14], 23, 0xFDE5380C);
        hh!(a, b, c, d, x[1], 4, 0xA4BEEA44);
        hh!(d, a, b, c, x[4], 11, 0x4BDECFA9);
        hh!(c, d, a, b, x[7], 16, 0xF6BB4B60);
        hh!(b, c, d, a, x[10], 23, 0xBEBFBC70);
        hh!(a, b, c, d, x[13], 4, 0x289B7EC6);
        hh!(d, a, b, c, x[0], 11, 0xEAA127FA);
        hh!(c, d, a, b, x[3], 16, 0xD4EF3085);
        hh!(b, c, d, a, x[6], 23, 0x04881D05);
        hh!(a, b, c, d, x[9], 4, 0xD9D4D039);
        hh!(d, a, b, c, x[12], 11, 0xE6DB99E5);
        hh!(c, d, a, b, x[15], 16, 0x1FA27CF8);
        hh!(b, c, d, a, x[2], 23, 0xC4AC5665);

        // Round 4
        macro_rules! ii {
            ($a:expr, $b:expr, $c:expr, $d:expr, $x:expr, $s:expr, $ac:expr) => {
                $a = $a.wrapping_add(($c ^ ($b | !$d)).wrapping_add($x).wrapping_add($ac));
                $a = $a.rotate_left($s).wrapping_add($b);
            };
        }

        ii!(a, b, c, d, x[0], 6, 0xF4292244);
        ii!(d, a, b, c, x[7], 10, 0x432AFF97);
        ii!(c, d, a, b, x[14], 15, 0xAB9423A7);
        ii!(b, c, d, a, x[5], 21, 0xFC93A039);
        ii!(a, b, c, d, x[12], 6, 0x655B59C3);
        ii!(d, a, b, c, x[3], 10, 0x8F0CCC92);
        ii!(c, d, a, b, x[10], 15, 0xFFEFF47D);
        ii!(b, c, d, a, x[1], 21, 0x85845DD1);
        ii!(a, b, c, d, x[8], 6, 0x6FA87E4F);
        ii!(d, a, b, c, x[15], 10, 0xFE2CE6E0);
        ii!(c, d, a, b, x[6], 15, 0xA3014314);
        ii!(b, c, d, a, x[13], 21, 0x4E0811A1);
        ii!(a, b, c, d, x[4], 6, 0xF7537E82);
        ii!(d, a, b, c, x[11], 10, 0xBD3AF235);
        ii!(c, d, a, b, x[2], 15, 0x2AD7D2BB);
        ii!(b, c, d, a, x[9], 21, 0xEB86D391);

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
    }

    fn finalize(&mut self) -> [u8; 16]
    {
        let mut bits = [0u8; 8];
        for i in 0..2
        {
            bits[i * 4..i * 4 + 4].copy_from_slice(&self.count[i].to_le_bytes());
        }

        // Pad to 56 bytes mod 64
        let index = (self.count[0] >> 3) as usize & 0x3F;
        let pad_len = if index < 56 { 56 - index } else { 120 - index };
        let mut padding = vec![0u8; pad_len];
        padding[0] = 0x80;
        self.update(&padding);

        // Append length before final transform
        self.update(&bits);

        // Store state in digest
        let mut digest = [0u8; 16];
        for i in 0..4
        {
            digest[i * 4..i * 4 + 4].copy_from_slice(&self.state[i].to_le_bytes());
        }
        digest
    }
}

/// Compute MD5 checksum of audio samples
fn compute_md5(samples: &[i16], channels: u16, bits_per_sample: u8) -> [u8; 16]
{
    let mut ctx = MD5Context::new();

    // Process samples in little-endian byte order
    // For FLAC, samples are interleaved and sign-extended if needed
    for &sample in samples
    {
        let bytes = sample.to_le_bytes();
        ctx.update(&bytes);
    }

    ctx.finalize()
}

/// Bit writer for FLAC encoding
struct BitWriter
{
    buffer: Vec<u8>,
    current_byte: u8,
    bit_count: u8,
}

impl BitWriter
{
    fn new() -> Self
    {
        BitWriter
        {
            buffer: Vec::new(),
            current_byte: 0,
            bit_count: 0,
        }
    }

    fn write_bits(&mut self, value: u64, bits: u8)
    {
        let mut bits_remaining = bits;

        while bits_remaining > 0
        {
            let bits_to_write = std::cmp::min(8 - self.bit_count, bits_remaining);
            let shift = bits_remaining - bits_to_write;

            // Handle the mask carefully to avoid overflow
            let mask = if bits_to_write >= 64
            {
                u64::MAX
            }
            else
            {
                ((1u64 << bits_to_write) - 1)
            };

            let bits_value = if shift >= 64
            {
                0u8
            }
            else
            {
                ((value >> shift) & mask) as u8
            };

            self.current_byte |= bits_value << (8 - self.bit_count - bits_to_write);
            self.bit_count += bits_to_write;

            if self.bit_count == 8
            {
                self.buffer.push(self.current_byte);
                self.current_byte = 0;
                self.bit_count = 0;
            }

            bits_remaining -= bits_to_write;
        }
    }

    fn write_byte(&mut self, byte: u8)
    {
        self.write_bits(byte as u64, 8);
    }

    fn write_bytes(&mut self, bytes: &[u8])
    {
        for &byte in bytes
        {
            self.write_byte(byte);
        }
    }

    fn write_unary(&mut self, value: u32)
    {
        // Write 'value' zeros followed by a one
        for _ in 0..value
        {
            self.write_bits(0, 1);
        }
        self.write_bits(1, 1);
    }

    fn byte_align(&mut self)
    {
        if self.bit_count > 0
        {
            self.buffer.push(self.current_byte);
            self.current_byte = 0;
            self.bit_count = 0;
        }
    }

    fn get_bytes(&self) -> Vec<u8>
    {
        let mut result = self.buffer.clone();
        if self.bit_count > 0
        {
            result.push(self.current_byte);
        }
        result
    }
}

/// Write UTF-8 encoded number for frame header
fn write_utf8_number(writer: &mut BitWriter, value: u64)
{
    if value < 0x80
    {
        writer.write_byte(value as u8);
    }
    else if value < 0x800
    {
        writer.write_byte(0xC0 | ((value >> 6) & 0x1F) as u8);
        writer.write_byte(0x80 | (value & 0x3F) as u8);
    }
    else if value < 0x10000
    {
        writer.write_byte(0xE0 | ((value >> 12) & 0x0F) as u8);
        writer.write_byte(0x80 | ((value >> 6) & 0x3F) as u8);
        writer.write_byte(0x80 | (value & 0x3F) as u8);
    }
    else if value < 0x200000
    {
        writer.write_byte(0xF0 | ((value >> 18) & 0x07) as u8);
        writer.write_byte(0x80 | ((value >> 12) & 0x3F) as u8);
        writer.write_byte(0x80 | ((value >> 6) & 0x3F) as u8);
        writer.write_byte(0x80 | (value & 0x3F) as u8);
    }
    else if value < 0x4000000
    {
        writer.write_byte(0xF8 | ((value >> 24) & 0x03) as u8);
        writer.write_byte(0x80 | ((value >> 18) & 0x3F) as u8);
        writer.write_byte(0x80 | ((value >> 12) & 0x3F) as u8);
        writer.write_byte(0x80 | ((value >> 6) & 0x3F) as u8);
        writer.write_byte(0x80 | (value & 0x3F) as u8);
    }
    else if value < 0x80000000
    {
        writer.write_byte(0xFC | ((value >> 30) & 0x01) as u8);
        writer.write_byte(0x80 | ((value >> 24) & 0x3F) as u8);
        writer.write_byte(0x80 | ((value >> 18) & 0x3F) as u8);
        writer.write_byte(0x80 | ((value >> 12) & 0x3F) as u8);
        writer.write_byte(0x80 | ((value >> 6) & 0x3F) as u8);
        writer.write_byte(0x80 | (value & 0x3F) as u8);
    }
    else
    {
        writer.write_byte(0xFE);
        writer.write_byte(0x80 | ((value >> 30) & 0x3F) as u8);
        writer.write_byte(0x80 | ((value >> 24) & 0x3F) as u8);
        writer.write_byte(0x80 | ((value >> 18) & 0x3F) as u8);
        writer.write_byte(0x80 | ((value >> 12) & 0x3F) as u8);
        writer.write_byte(0x80 | ((value >> 6) & 0x3F) as u8);
        writer.write_byte(0x80 | (value & 0x3F) as u8);
    }
}

/// Apply fixed predictor of given order
fn apply_fixed_predictor(samples: &[i32], order: usize) -> Vec<i32>
{
    let mut residual = Vec::with_capacity(samples.len());

    // For fixed predictor, the entire residual array is returned
    // but only samples after 'order' are actual residuals

    // Apply predictor to all samples
    for i in 0..samples.len()
    {
        if i < order
        {
            // Warm-up samples aren't predicted, they're stored in the subframe header
            residual.push(0); // These will be ignored during encoding
        }
        else
        {
            let predicted = match order
            {
                0 => 0,
                1 => samples[i - 1],
                2 => 2 * samples[i - 1] - samples[i - 2],
                3 => 3 * samples[i - 1] - 3 * samples[i - 2] + samples[i - 3],
                4 => 4 * samples[i - 1] - 6 * samples[i - 2] + 4 * samples[i - 3] - samples[i - 4],
                _ => return residual, // Invalid order
            };
            residual.push(samples[i] - predicted);
        }
    }

    residual
}

/// Calculate best Rice parameter for a partition
fn calculate_rice_parameter(residual: &[i32]) -> u32
{
    if residual.is_empty()
    {
        return 0;
    }

    // Calculate mean of absolute values
    let sum: u64 = residual.iter().map(|&x| x.unsigned_abs() as u64).sum();
    let mean = sum / residual.len() as u64;

    // Estimate Rice parameter based on mean absolute deviation
    // The optimal parameter is roughly log2(mean * 0.75)
    if mean == 0
    {
        return 0;
    }

    let mut param = 0u32;
    let mut test_mean = mean;

    while test_mean > 0 && param < MAX_RICE_PARAM_4BIT
    {
        test_mean >>= 1;
        if test_mean > 0
        {
            param += 1;
        }
    }

    // Adjust for better compression
    if param > 0 && mean < (1 << (param - 1))
    {
        param -= 1;
    }

    param.min(MAX_RICE_PARAM_4BIT)
}

/// Encode residual using Rice coding
fn encode_rice_partition(writer: &mut BitWriter, residual: &[i32], rice_param: u32) -> Result<()>
{
    for &sample in residual
    {
        // Zigzag encode (fold) the residual - mapping signed to unsigned
        let folded = if sample >= 0
        {
            (sample as u32) << 1
        }
        else
        {
            (((-(sample + 1)) as u32) << 1) | 1
        };

        // Split into MSB and LSB parts
        let msb = folded >> rice_param;
        let lsb = folded & ((1 << rice_param) - 1);

        // Write unary-encoded MSB (zeros followed by a one)
        writer.write_unary(msb);

        // Write binary-encoded LSB
        if rice_param > 0
        {
            writer.write_bits(lsb as u64, rice_param as u8);
        }
    }

    Ok(())
}

/// Encode residual with partitioned Rice coding
fn encode_residual(writer: &mut BitWriter, residual: &[i32], predictor_order: usize, block_size: usize, compression_level: u8) -> Result<()>
{
    // Calculate partition order based on compression level
    let mut partition_order = match compression_level
    {
        0 => 0,
        1..=2 => 2.min((block_size.trailing_zeros()).min(8)),
        3..=5 => 4.min((block_size.trailing_zeros()).min(8)),
        6..=8 => 6.min((block_size.trailing_zeros()).min(8)),
        _ => 6.min((block_size.trailing_zeros()).min(8)),
    };

    // Ensure valid partition order
    while partition_order > 0
    {
        let partition_samples = block_size >> partition_order;
        if partition_samples > predictor_order && partition_samples >= 4
        {
            break;
        }
        partition_order -= 1;
    }

    // Write coding method (0b00 for 4-bit Rice parameters)
    writer.write_bits(0, 2);

    // Write partition order
    writer.write_bits(partition_order as u64, 4);

    let num_partitions = 1 << partition_order;
    let default_partition_samples = block_size >> partition_order;

    let mut sample_idx = 0;
    for partition_idx in 0..num_partitions
    {
        // First partition has fewer samples due to predictor order
        let partition_samples = if partition_idx == 0
        {
            default_partition_samples - predictor_order
        }
        else
        {
            default_partition_samples
        };

        if partition_samples == 0
        {
            continue;
        }

        let partition_residual = &residual[sample_idx..sample_idx + partition_samples];
        sample_idx += partition_samples;

        // Calculate best Rice parameter for this partition
        let rice_param = calculate_rice_parameter(partition_residual);

        if rice_param > MAX_RICE_PARAM_4BIT
        {
            // Use escape code for incompressible data
            writer.write_bits(0xF, 4); // Escape code (all ones)

            // Calculate bits needed for raw samples
            let mut max_val = 0u32;
            for &sample in partition_residual
            {
                max_val = max_val.max(sample.unsigned_abs());
            }

            let mut bits_needed = 1u32; // At least 1 bit for sign
            while (1u32 << bits_needed) <= max_val && bits_needed < 32
            {
                bits_needed += 1;
            }
            bits_needed += 1; // Add sign bit
            bits_needed = bits_needed.max(1).min(32);

            // Write bits per sample minus 1
            writer.write_bits((bits_needed - 1) as u64, 5);

            // Write samples as raw signed integers
            for &sample in partition_residual
            {
                // Write as signed value with calculated bit width
                writer.write_bits(sample as u32 as u64, bits_needed as u8);
            }
        }
        else
        {
            // Write Rice parameter
            writer.write_bits(rice_param as u64, 4);

            // Encode partition with Rice coding
            encode_rice_partition(writer, partition_residual, rice_param)?;
        }
    }

    Ok(())
}

/// Encode a subframe
fn encode_subframe(writer: &mut BitWriter, samples: &[i32], bits_per_sample: u8, compression_level: u8) -> Result<()>
{
    let block_size = samples.len();

    // Choose predictor order based on compression level
    let predictor_order = match compression_level
    {
        0 => 0, // Verbatim (no prediction)
        1 => if block_size >= 1 { 1 } else { 0 },
        2 => if block_size >= 2 { 2 } else { 0 },
        3..=4 => if block_size >= 3 { 3 } else { 0 },
        5..=8 => if block_size >= 4 { 4 } else { 0 },
        _ => if block_size >= 4 { 4 } else { 0 },
    };

    // Write subframe header
    // Bits 0: Zero bit
    writer.write_bits(0, 1);

    // Bits 1-6: Subframe type
    if predictor_order == 0
    {
        // Verbatim subframe
        writer.write_bits(0b000001, 6);
    }
    else
    {
        // Fixed predictor subframe
        let subframe_type = 0b001000 | (predictor_order as u64);
        writer.write_bits(subframe_type as u64, 6);
    }

    // Bit 7: No wasted bits
    writer.write_bits(0, 1);

    if predictor_order == 0
    {
        // Verbatim subframe - write samples directly
        for &sample in samples
        {
            writer.write_bits(sample as u64, bits_per_sample);
        }
    }
    else
    {
        // Write warm-up samples
        for i in 0..predictor_order
        {
            writer.write_bits(samples[i] as u64, bits_per_sample);
        }

        // Calculate and encode residual
        let residual = apply_fixed_predictor(samples, predictor_order);
        // Pass the entire residual but encoding starts after warm-up samples
        encode_residual(writer, &residual[predictor_order..], predictor_order, block_size, compression_level)?;
    }

    Ok(())
}

/// Encode a frame
fn encode_frame(
    writer: &mut BitWriter,
    samples: &[i16],
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u8,
    frame_number: u32,
    block_size: usize,
    compression_level: u8,
) -> Result<()>
{
    let frame_start = writer.buffer.len();

    // Frame header
    // Sync code: 0b11111111111111 (14 bits)
    writer.write_bits(FRAME_SYNC_CODE as u64, 14);

    // Reserved bit: 0
    writer.write_bits(0, 1);

    // Blocking strategy: 0 (fixed block size)
    writer.write_bits(0, 1);

    // Block size bits
    let block_size_bits = match block_size
    {
        192 => 0b0001,
        576 => 0b0010,
        1152 => 0b0011,
        2304 => 0b0100,
        4608 => 0b0101,
        256 => 0b1000,
        512 => 0b1001,
        1024 => 0b1010,
        2048 => 0b1011,
        4096 => 0b1100,
        8192 => 0b1101,
        16384 => 0b1110,
        32768 => 0b1111,
        _ =>
            {
                // Uncommon block size
                if block_size < 256
                {
                    0b0110
                }
                else
                {
                    0b0111
                }
            }
    };
    writer.write_bits(block_size_bits, 4);

    // Sample rate bits
    let sample_rate_bits = match sample_rate
    {
        88200 => 0b0001,
        176400 => 0b0010,
        192000 => 0b0011,
        8000 => 0b0100,
        16000 => 0b0101,
        22050 => 0b0110,
        24000 => 0b0111,
        32000 => 0b1000,
        44100 => 0b1001,
        48000 => 0b1010,
        96000 => 0b1011,
        _ => 0b0000, // Get from streaminfo
    };
    writer.write_bits(sample_rate_bits, 4);

    // Channel assignment
    let channel_bits = if channels == 1
    {
        0b0000 // Mono
    }
    else if channels == 2
    {
        0b0001 // Stereo (left, right)
    }
    else
    {
        (channels - 1) as u32 // Multi-channel
    };
    writer.write_bits(channel_bits as u64, 4);

    // Sample size bits
    let sample_size_bits = match bits_per_sample
    {
        8 => 0b001,
        12 => 0b010,
        16 => 0b100,
        20 => 0b101,
        24 => 0b110,
        _ => 0b000, // Get from streaminfo
    };
    writer.write_bits(sample_size_bits, 3);

    // Reserved bit: 0
    writer.write_bits(0, 1);

    // Frame/sample number (UTF-8 encoded)
    write_utf8_number(writer, frame_number as u64);

    // Uncommon block size (if needed)
    if block_size_bits == 0b0110
    {
        writer.write_byte((block_size - 1) as u8);
    }
    else if block_size_bits == 0b0111
    {
        writer.write_bits((block_size - 1) as u64, 16);
    }

    // Frame header CRC-8
    // We need to get all header bytes including any partial byte
    let mut header_bytes = writer.buffer[frame_start..].to_vec();
    if writer.bit_count > 0
    {
        header_bytes.push(writer.current_byte);
    }
    let crc8_value = crc8(&header_bytes);
    writer.write_byte(crc8_value);

    // Encode subframes
    let mut channel_samples = vec![vec![0i32; block_size]; channels as usize];

    // Deinterleave samples
    for i in 0..block_size
    {
        for ch in 0..channels as usize
        {
            let sample_idx = i * channels as usize + ch;
            if sample_idx < samples.len()
            {
                channel_samples[ch][i] = samples[sample_idx] as i32;
            }
        }
    }

    // Encode each channel
    for ch in 0..channels as usize
    {
        encode_subframe(writer, &channel_samples[ch], bits_per_sample, compression_level)?;
    }

    // Byte-align
    writer.byte_align();

    // Frame footer (CRC-16)
    // CRC-16 covers the entire frame from sync code to just before the CRC itself
    let frame_bytes = &writer.buffer[frame_start..];
    let crc16_value = crc16(frame_bytes);
    writer.write_bits(crc16_value as u64, 16);

    Ok(())
}

/// Write streaminfo metadata block
fn write_streaminfo(
    writer: &mut BitWriter,
    min_block_size: u16,
    max_block_size: u16,
    min_frame_size: u32,
    max_frame_size: u32,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u8,
    total_samples: u64,
    md5: [u8; 16],
)
{
    // Metadata block header
    // Last metadata block flag: 1
    writer.write_bits(1, 1);
    // Block type: 0 (streaminfo)
    writer.write_bits(0, 7);
    // Length: 34 bytes
    writer.write_bits(34, 24);

    // Streaminfo data
    writer.write_bits(min_block_size as u64, 16);
    writer.write_bits(max_block_size as u64, 16);
    writer.write_bits(min_frame_size as u64, 24);
    writer.write_bits(max_frame_size as u64, 24);
    writer.write_bits(sample_rate as u64, 20);
    writer.write_bits((channels - 1) as u64, 3);
    writer.write_bits((bits_per_sample - 1) as u64, 5);
    writer.write_bits(total_samples, 36);

    // MD5 checksum
    for byte in md5
    {
        writer.write_byte(byte);
    }
}

/// Main FLAC encoding function with compression level
pub fn encode_flac_with_level(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
    compression_level: u8,
) -> Result<Vec<u8>>
{
    // Convert f32 samples to i16
    let i16_samples: Vec<i16> = samples
        .iter()
        .map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16)
        .collect();

    let total_samples = i16_samples.len() / channels as usize;

    // FLAC requires at least 16 samples per channel
    if total_samples < 16
    {
        return Err(anyhow!(
            "FLAC requires at least 16 samples per channel, got {}",
            total_samples
        ));
    }

    // Validate compression level
    if compression_level > 8
    {
        return Err(anyhow!(
            "Invalid compression level {}, must be 0-8",
            compression_level
        ));
    }

    let bits_per_sample = 16u8;

    // Choose block size based on compression level
    let block_size = match compression_level
    {
        0 => 1152,  // Fast encoding
        1 => 1152,
        2 => 1152,
        3 => 4096,
        4 => 4096,
        5 => 4096,  // Default
        6 => 4096,
        7 => 4096,
        8 => 4096,  // Maximum compression
        _ => 4096,
    }.min(total_samples).max(16);


    let mut writer = BitWriter::new();

    // Write FLAC signature
    writer.write_bytes(&FLAC_SIGNATURE);

    // Calculate MD5 checksum of audio data
    let md5 = compute_md5(&i16_samples, channels, bits_per_sample);

    // Write streaminfo
    write_streaminfo(
        &mut writer,
        block_size as u16,
        block_size as u16,
        0, // Unknown min frame size
        0, // Unknown max frame size
        sample_rate,
        channels,
        bits_per_sample,
        total_samples as u64,
        md5,
    );

    // Encode frames
    let mut sample_offset = 0;
    let mut frame_number = 0u32;

    while sample_offset < i16_samples.len()
    {
        let remaining = i16_samples.len() - sample_offset;
        let current_block_size = block_size.min(remaining / channels as usize);

        if current_block_size == 0
        {
            break;
        }

        let frame_samples = &i16_samples[sample_offset..sample_offset + current_block_size * channels as usize];

        encode_frame(
            &mut writer,
            frame_samples,
            channels,
            sample_rate,
            bits_per_sample,
            frame_number,
            current_block_size,
            compression_level,
        )?;

        sample_offset += current_block_size * channels as usize;
        frame_number += 1;
    }

    Ok(writer.get_bytes())
}

/// Main FLAC encoding function with default compression level 5
pub fn encode_flac(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<Vec<u8>>
{
    encode_flac_with_level(samples, sample_rate, channels, 5)
}

/// Export audio to FLAC file with specific compression level
pub fn export_to_flac_with_level(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
    compression_level: u8,
) -> Result<()>
{
    let flac_data = encode_flac_with_level(samples, sample_rate, channels, compression_level)?;
    let mut file = std::fs::File::create(path)?;
    file.write_all(&flac_data)?;
    Ok(())
}

/// Export audio to FLAC file with default compression level 5
pub fn export_to_flac(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<()>
{
    export_to_flac_with_level(path, samples, sample_rate, channels, 5)
}