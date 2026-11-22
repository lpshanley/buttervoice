use buttervoice_core::capture::{AudioFormat, CapturedAudio};

fn audio(sample_rate: u32, channels: u16, samples: &[f32]) -> CapturedAudio {
    CapturedAudio {
        format: AudioFormat {
            sample_rate,
            channels,
        },
        samples: samples.to_vec(),
        dropped_samples: 0,
    }
}

// Expected bytes are written out by hand from the WAV format: a 44-byte
// header followed by little-endian 16-bit samples.
#[test]
fn mono_audio_becomes_a_16_bit_pcm_wav_file() {
    let wav = audio(8_000, 1, &[0.0, 0.5, -1.0, 1.0]).to_wav();

    #[rustfmt::skip]
    let expected: &[u8] = &[
        b'R', b'I', b'F', b'F', 44, 0, 0, 0, b'W', b'A', b'V', b'E',
        b'f', b'm', b't', b' ', 16, 0, 0, 0,
        1, 0,                   // PCM
        1, 0,                   // one channel
        0x40, 0x1F, 0, 0,       // 8,000 samples a second
        0x80, 0x3E, 0, 0,       // 16,000 bytes a second
        2, 0,                   // bytes per frame
        16, 0,                  // bits per sample
        b'd', b'a', b't', b'a', 8, 0, 0, 0,
        0x00, 0x00,             //  0.0
        0x00, 0x40,             //  0.5
        0x01, 0x80,             // -1.0
        0xFF, 0x7F,             //  1.0
    ];
    assert_eq!(wav, expected);
}

#[test]
fn stereo_audio_is_described_as_two_channels() {
    let wav = audio(48_000, 2, &[0.0, 0.0]).to_wav();

    assert_eq!(&wav[22..24], [2, 0], "channels");
    assert_eq!(
        &wav[28..32],
        [0x00, 0xEE, 0x02, 0x00],
        "192,000 bytes a second"
    );
    assert_eq!(&wav[32..34], [4, 0], "bytes per frame");
}

#[test]
fn samples_beyond_full_scale_are_clipped_instead_of_wrapping() {
    let wav = audio(8_000, 1, &[1.5, -2.0]).to_wav();

    assert_eq!(&wav[44..], [0xFF, 0x7F, 0x01, 0x80]);
}
