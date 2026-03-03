pub mod models;

#[cfg(test)]
mod tests {
    use crate::models::melspectrogram::MelspectrogramModel;
    use ndarray::Array1;

    fn generate_sine(freq: f64, sample_rate: usize, duration: f64) -> Array1<i16> {
        let samples = (sample_rate as f64 * duration) as usize;
        Array1::from_iter((0..samples).map(|i| {
            let t = i as f64 / sample_rate as f64;
            (f64::sin(2.0 * std::f64::consts::PI * freq * t) * 32767.0) as i16
        }))
    }

    #[test]
    fn test_melspectrogram_output_shape() {
        let mut model = MelspectrogramModel::new("models/melspectrogram.onnx").unwrap();

        let audio_1s = generate_sine(440.0, 16000, 1.0);
        let audio_2s = generate_sine(440.0, 16000, 2.0);

        let output_1s = model.detect(&audio_1s).unwrap();
        let output_2s = model.detect(&audio_2s).unwrap();

        // mel_bins is always 32
        assert_eq!(output_1s.shape()[1], 32);
        assert_eq!(output_2s.shape()[1], 32);

        // longer audio should produce more time frames
        assert!(output_2s.shape()[0] > output_1s.shape()[0]);
    }
}
