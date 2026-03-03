pub mod melspectrogram;
pub mod embedding;
mod wakeword;

#[cfg(test)]
mod tests {
    use crate::melspectrogram::MelspectrogramModel;
    use crate::embedding::EmbeddingModel;
    use ndarray::Array1;

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
    fn test_mel_to_embedding_pipeline() {
        let mut mel_model = MelspectrogramModel::new().unwrap();
        let mut emb_model = EmbeddingModel::new().unwrap();

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
