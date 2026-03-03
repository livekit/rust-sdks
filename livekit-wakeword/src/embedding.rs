use ndarray::{Array1, Array2, Axis};
use ort::session::Session;
use ort::value::Tensor;

use crate::build_session_from_memory;

const MODEL_BYTES: &[u8] = include_bytes!("../onnx/embedding_model.onnx");

// Produces a 96-dim embedding from mel spectrogram features using a pre-trained ONNX model.
//
// Model input:  f32 tensor of shape (batch, 76, 32, 1) — mel spectrogram features
// Model output: f32 tensor of shape (batch, 1, 1, 96) — embedding vector
//
// The detect() method accepts mel spectrogram features as an Array2<f32> of shape (76, 32),
// and returns the embedding as an Array1<f32> of length 96.
pub struct EmbeddingModel {
    session: Session,
}

impl EmbeddingModel {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { session: build_session_from_memory(MODEL_BYTES)? })
    }

    // Run the embedding model on mel spectrogram features and return the embedding.
    // Input: Array2<f32> of shape (76, 32) — mel spectrogram features.
    // Output: Array1<f32> of length 96 — embedding vector.
    pub fn detect(
        &mut self,
        mel_features: &Array2<f32>,
    ) -> Result<Array1<f32>, Box<dyn std::error::Error>> {
        let input = mel_features.clone().insert_axis(Axis(0)).insert_axis(Axis(3));
        let tensor = Tensor::from_array(input)?;

        let outputs = self.session.run(ort::inputs![tensor])?;

        let raw = outputs["conv2d_19"].try_extract_array::<f32>()?;
        let embedding = raw.into_owned().into_shape_with_order(crate::EMBEDDING_DIM)?;

        Ok(embedding)
    }
}
