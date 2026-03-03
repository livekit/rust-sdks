pub mod models;

#[cfg(test)]
mod tests {
    use crate::models::melspectrogram::MelspectrogramModel;
    use crate::models::embedding::EmbeddingModel;
    use ndarray::{ Array1, Array2 };

    fn generate_sine(freq: f64, sample_rate: usize, duration: f64) -> Array1<i16> {
        let samples = ((sample_rate as f64) * duration) as usize;
        Array1::from_iter(
            (0..samples).map(|i| {
                let t = (i as f64) / (sample_rate as f64);
                (f64::sin(2.0 * std::f64::consts::PI * freq * t) * 32767.0) as i16
            })
        )
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

    #[test]
    fn test_embedding_output_shape() {
        let mut model = EmbeddingModel::new("models/embedding_model.onnx").unwrap();

        // Create a dummy mel spectrogram input of shape (76, 32)
        let mel_features = Array2::<f32>::zeros((76, 32));
        let output = model.detect(&mel_features).unwrap();

        // Output should be a 96-dim embedding vector
        assert_eq!(output.shape(), &[96]);
    }

    #[test]
    fn test_mel_to_embedding_pipeline() {
        let mut mel_model = MelspectrogramModel::new("models/melspectrogram.onnx").unwrap();
        let mut emb_model = EmbeddingModel::new("models/embedding_model.onnx").unwrap();

        // Generate two different audio signals
        let audio_a = generate_sine(440.0, 16000, 1.0);
        let audio_b = generate_sine(880.0, 16000, 1.0);

        // Run mel spectrogram
        let mel_a = mel_model.detect(&audio_a).unwrap();
        let mel_b = mel_model.detect(&audio_b).unwrap();

        // mel output should have 32 mel bins
        assert_eq!(mel_a.shape()[1], 32);
        assert_eq!(mel_b.shape()[1], 32);

        // Slice to (76, 32) for the embedding model
        let mel_a_sliced = mel_a.slice(ndarray::s![..76, ..]).to_owned();
        let mel_b_sliced = mel_b.slice(ndarray::s![..76, ..]).to_owned();

        // Run embedding
        let embed_a = emb_model.detect(&mel_a_sliced).unwrap();
        let embed_b = emb_model.detect(&mel_b_sliced).unwrap();

        // Both should be 96-dim embeddings
        assert_eq!(embed_a.shape(), &[96]);
        assert_eq!(embed_b.shape(), &[96]);

        // Different audio should produce different embeddings
        assert_ne!(embed_a, embed_b);
    }
}
