use crate::models::prelude::*;

// Extracts mel-scaled spectrogram features from raw audio using a pre-trained ONNX model.
//
// Model input:  f32 tensor of shape (1, num_samples) — mono PCM audio normalized to [-1.0, 1.0]
// Model output: f32 tensor of shape (1, 1, time_frames, mel_bins) — e.g. (1, 1, 97, 32) for 16000 samples
//
// The detect() method accepts i16 PCM samples, handles normalization internally,
// and returns the mel features as an Array2<f32> of shape (time_frames, mel_bins).
pub struct MelspectrogramModel {
    session: Session,
}

impl MelspectrogramModel {
    // Initialize the melspectrogram model from the given file path
    // The model file is expected to be in ONNX format
    pub fn new(model_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(model_path)?;

        Ok(Self { session })
    }

    // Run the melspectrogram model on raw audio and return mel features.
    // Input: 1D array of i16 PCM samples.
    // Output: Array2<f32> of shape (time_frames, mel_bins) e.g. (97, 32).
    pub fn detect(
        &mut self,
        audio: &Array1<i16>
    ) -> Result<Array2<f32>, Box<dyn std::error::Error>> {
        // Convert i16 samples to f32 in [-1.0, 1.0]
        let audio_f32: Array1<f32> = audio
            .iter()
            .map(|&x| (x as f32) / 32768.0)
            .collect();

        // Model expects shape (1, num_samples), add batch dimension
        let audio_2d = audio_f32.insert_axis(Axis(0));
        let audio_tensor = Tensor::from_array(audio_2d)?;

        let inputs = ort::inputs![audio_tensor];
        let features = self.session.run(inputs)?;

        // Raw output shape is [1, 1, time_frames, mel_bins] — reshape to 2D
        let raw = features["output"].try_extract_array::<f32>()?;
        let rows = raw.shape()[2];
        let cols = raw.shape()[3];
        let output = raw.into_owned().into_shape_with_order((rows, cols))?;

        Ok(output)
    }
}
