// Sondas da revisão independente de F01/F02; não são testes de aceite.
// Vincular ao seele_audio compilado da árvore atual. Não acessa dispositivos.
use seele_audio::gate::{GateConfig, GateMode, VoiceGate};
use seele_audio::jitter::{JitterBuffer, JitterConfig};
use seele_audio::supressao::Supressao;
use seele_audio::FRAME_SAMPLES;

fn noise(seed: &mut u32) -> Vec<f32> {
    (0..FRAME_SAMPLES).map(|_| {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        0.01 * ((*seed >> 8) as f32 / (1_u32 << 24) as f32 * 2.0 - 1.0)
    }).collect()
}

fn main() {
    let mut filter = Supressao::nova(1.0);
    let mut out = Vec::new();
    let mut seed = 7;
    filter.processar(&noise(&mut seed), &mut out);
    println!("FIRST_OUTPUT samples={} encoder_expected={}", out.len(), FRAME_SAMPLES);
    assert_eq!(out.len(), FRAME_SAMPLES / 2);
    let mut encoder = seele_audio::codec::VoiceEncoder::with_defaults().unwrap();
    let encoded = encoder.encode(&out);
    assert!(matches!(encoded, Err(seele_audio::codec::CodecError::WrongFrameSize {got:480, expected:960})));
    println!("FIRST_OUTPUT_CODEC {encoded:?}");

    // The live loop calls set_mode on every iteration even when unchanged.
    let mut gate = VoiceGate::new(GateConfig::de_dbfs(-60.0), GateMode::VoiceActivated);
    let mut frames = Vec::new();
    for amplitude in [0.0001, 0.0002, 0.02] {
        gate.set_mode(GateMode::VoiceActivated);
        gate.quadros_a_transmitir(&vec![amplitude; FRAME_SAMPLES], &mut frames);
    }
    println!("MODE_RESET opening_frames={} retained={}", frames.len(), gate.metrics().quadros_retidos_entregues);
    assert_eq!(frames.len(), 1);
    gate.set_mode(GateMode::VoiceActivated);
    gate.quadros_a_transmitir(&vec![0.0; FRAME_SAMPLES], &mut frames);
    println!("MODE_RESET first_quiet_frame_open={}", !frames.is_empty());
    assert!(frames.is_empty());

    let mut filter = Supressao::nova(1.0);
    let mut gate = VoiceGate::new(GateConfig::de_dbfs(-60.0), GateMode::VoiceActivated);
    let mut opened = 0;
    let mut level = 0.0_f32;
    for i in 0..250 {
        filter.processar(&noise(&mut seed), &mut out);
        let speaking = gate.update(&out);
        if i >= 50 { opened += usize::from(speaking); level += VoiceGate::rms(&out); }
    }
    println!("NOISE_ONLY after_warmup_open={opened}/200 mean_output_dbfs={:.2}", 20.0*(level/200.0).log10());
    assert!(opened > 190, "the current defect no longer reproduces");

    let mut filter = Supressao::nova(1.0);
    for _ in 0..25 { filter.processar(&vec![0.0; FRAME_SAMPLES], &mut out); }
    let mut input_rms = 0.0;
    let mut output_rms = 0.0;
    for i in 0..250 {
        let input = noise(&mut seed);
        filter.processar(&input, &mut out);
        if i >= 50 { input_rms += VoiceGate::rms(&input); output_rms += VoiceGate::rms(&out); }
    }
    println!("SILENT_START noise_remaining_ratio={:.4}", output_rms/input_rms);
    assert!(output_rms/input_rms > 0.99);

    // Reproduce the sender's timestamp assignment with actual gate output,
    // then hand those headers to the actual jitter buffer. No network/codec here.
    let mut gate = VoiceGate::new(GateConfig::de_dbfs(-60.0), GateMode::VoiceActivated);
    let mut frames = Vec::new();
    let mut timestamp = 0_u32;
    for amplitude in [0.0001, 0.0002, 0.02] {
        gate.quadros_a_transmitir(&vec![amplitude; FRAME_SAMPLES], &mut frames);
        timestamp += FRAME_SAMPLES as u32;
    }
    assert_eq!(frames.len(), 3);
    let mut jitter = JitterBuffer::new(JitterConfig {
        initial_target_ms:20.0, min_target_ms:20.0, max_target_ms:20.0,
        ..JitterConfig::default()
    });
    for (i, _) in frames.iter().enumerate() { jitter.push(i as u16 + 1, timestamp, 60.0, i); }
    jitter.push(4, timestamp + FRAME_SAMPLES as u32, 80.0, 3);
    let decisions: Vec<_> = (0..4).map(|_|jitter.tick()).collect();
    println!("RETAINED_FRAMES timestamps={:?} decisions={decisions:?} resyncs={}", vec![timestamp;frames.len()], jitter.metrics().resyncs);
    assert_eq!(jitter.metrics().resyncs, 2);
}
