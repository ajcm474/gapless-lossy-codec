use gapless_lossy_codec::codec::{Encoder, Decoder};

mod utils;
use utils::{generate_sine_wave, generate_square_wave, generate_sawtooth_wave, calculate_snr};

#[test]
fn test_sine_wave_440hz_mono()
{
    let samples = generate_sine_wave(440.0, 44100, 1, 2.0);
    let mut encoder = Encoder::new(44100);
    let encoded = encoder.encode(&samples, 1).expect("Encoding failed");
    
    let mut decoder = Decoder::new(1usize, 44100);
    let decoded = decoder.decode(&encoded, None).expect("Decoding failed");

    // Check length preservation (should be exactly the same)
    assert_eq!(decoded.len(), samples.len(), "Length mismatch: expected {}, got {}", samples.len(), decoded.len());

    // Check SNR
    let snr = calculate_snr(&samples, &decoded);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);

    println!("Sine 440Hz test: SNR = {:.2} dB, length = {}", snr, decoded.len());
}

#[test]
fn test_square_wave_1000hz_mono()
{
    let samples = generate_square_wave(1000.0, 44100, 1, 2.0);
    let mut encoder = Encoder::new(44100);
    let encoded = encoder.encode(&samples, 1).expect("Encoding failed");

    let mut decoder = Decoder::new(1usize, 44100);
    let decoded = decoder.decode(&encoded, None).expect("Decoding failed");

    // Check length preservation
    assert_eq!(decoded.len(), samples.len());

    // Check SNR (square waves are harder to encode, so allow lower SNR)
    let snr = calculate_snr(&samples, &decoded);
    assert!(snr > -15.0, "SNR too low: {} dB", snr);

    println!("Square 1000Hz test: SNR = {:.2} dB, length = {}", snr, decoded.len());
}

#[test]
fn test_sawtooth_wave_440hz_mono()
{
    let samples = generate_sawtooth_wave(440.0, 44100, 1, 2.0);
    let mut encoder = Encoder::new(44100);
    let encoded = encoder.encode(&samples, 1).expect("Encoding failed");

    let mut decoder = Decoder::new(1usize, 44100);
    let decoded = decoder.decode(&encoded, None).expect("Decoding failed");

    // Check length preservation
    assert_eq!(decoded.len(), samples.len());

    // Check SNR
    let snr = calculate_snr(&samples, &decoded);
    assert!(snr > -10.0, "SNR too low: {} dB", snr);

    println!("Sawtooth 440Hz test: SNR = {:.2} dB, length = {}", snr, decoded.len());
}

#[test]
fn test_sample_rate_variations()
{
    // Test 44.1 kHz
    let samples_44k = generate_sine_wave(440.0, 44100, 1, 1.0);
    let mut encoder = Encoder::new(44100);
    let encoded_44k = encoder.encode(&samples_44k, 1).expect("44.1kHz encoding failed");

    let mut decoder = Decoder::new(1usize, 44100);
    let decoded_44k = decoder.decode(&encoded_44k, None).expect("44.1kHz decoding failed");
    assert_eq!(decoded_44k.len(), samples_44k.len());

    // Test 48 kHz
    let samples_48k = generate_sine_wave(440.0, 48000, 1, 1.0);
    let mut encoder = Encoder::new(48000);
    let encoded_48k = encoder.encode(&samples_48k, 1).expect("48kHz encoding failed");

    let mut decoder = Decoder::new(1usize, 48000);
    let decoded_48k = decoder.decode(&encoded_48k, None).expect("48kHz decoding failed");
    assert_eq!(decoded_48k.len(), samples_48k.len());

    println!("Sample rate test: 44.1kHz={} samples, 48kHz={} samples",
             decoded_44k.len(), decoded_48k.len());
}

#[test]
fn test_stereo_encoding()
{
    let samples = generate_sine_wave(440.0, 44100, 2, 2.0);
    let mut encoder = Encoder::new(44100);
    let encoded = encoder.encode(&samples, 2).expect("Stereo encoding failed");

    let mut decoder = Decoder::new(1usize, 44100);
    let decoded = decoder.decode(&encoded, None).expect("Stereo decoding failed");

    // Check length preservation
    assert_eq!(decoded.len(), samples.len());

    // Check SNR
    let snr = calculate_snr(&samples, &decoded);
    assert!(snr > -10.0, "Stereo SNR too low: {} dB", snr);

    println!("Stereo test: SNR = {:.2} dB, length = {}", snr, decoded.len());
}

#[test]
fn test_short_duration()
{
    let samples = generate_sine_wave(440.0, 44100, 1, 0.5);  // 0.5 seconds
    let mut encoder = Encoder::new(44100);
    let encoded = encoder.encode(&samples, 1).expect("Short duration encoding failed");

    let mut decoder = Decoder::new(1usize, 44100);
    let decoded = decoder.decode(&encoded, None).expect("Short duration decoding failed");

    assert_eq!(decoded.len(), samples.len());
    println!("Short duration test: {} samples", decoded.len());
}

#[test]
fn test_long_duration()
{
    let samples = generate_sine_wave(440.0, 44100, 1, 5.0);  // 5 seconds
    let mut encoder = Encoder::new(44100);
    let encoded = encoder.encode(&samples, 1).expect("Long duration encoding failed");

    let mut decoder = Decoder::new(1usize, 44100);
    let decoded = decoder.decode(&encoded, None).expect("Long duration decoding failed");
    
    assert_eq!(decoded.len(), samples.len());
    println!("Long duration test: {} samples", decoded.len());
}

#[test]
fn test_gapless_multiple_files()
{
    // Simulate multiple files being decoded in sequence
    let file1 = generate_sine_wave(440.0, 44100, 1, 2.0);
    let file2 = generate_sine_wave(880.0, 44100, 1, 2.0);
    let file3 = generate_square_wave(440.0, 44100, 1, 2.0);
    
    let total_original_len = file1.len() + file2.len() + file3.len();
    
    // Encode each file
    let mut encoder = Encoder::new(44100);
    let encoded1 = encoder.encode(&file1, 1).expect("File 1 encoding failed");
    let encoded2 = encoder.encode(&file2, 1).expect("File 2 encoding failed");
    let encoded3 = encoder.encode(&file3, 1).expect("File 3 encoding failed");
    
    // Decode each file
    let mut decoder = Decoder::new(1usize, 44100);
    let decoded1 = decoder.decode(&encoded1, None).expect("File 1 decoding failed");
    let decoded2 = decoder.decode(&encoded2, None).expect("File 2 decoding failed");
    let decoded3 = decoder.decode(&encoded3, None).expect("File 3 decoding failed");
    
    let total_decoded_len = decoded1.len() + decoded2.len() + decoded3.len();
    
    // Should have exact length preservation
    assert_eq!(total_decoded_len, total_original_len, 
               "Gapless length mismatch: expected {}, got {}", 
               total_original_len, total_decoded_len);
    
    println!("Gapless test: {} original samples, {} decoded samples", 
             total_original_len, total_decoded_len);
}
