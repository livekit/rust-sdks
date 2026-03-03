use std::collections::HashMap;
use std::path::Path;

use ndarray::{Array1, Axis};
use ort::session::Session;
use ort::value::Tensor;

use crate::embedding::EmbeddingModel;
use crate::melspectrogram::MelspectrogramModel;
use crate::{build_session_from_file, EMBEDDING_STRIDE, EMBEDDING_WINDOW, MIN_EMBEDDINGS};

/// Stateless wake word detection model.
///
/// The mel spectrogram and speech embedding models are bundled at compile time.
/// Wake word classifier models are loaded dynamically from disk at runtime.
///
/// Pass ~2 seconds of 16 kHz i16 PCM audio to [`predict`](Self::predict) and
/// receive per-classifier confidence scores.
pub struct WakeWordModel {
    mel_model: MelspectrogramModel,
    emb_model: EmbeddingModel,
    classifiers: HashMap<String, Session>,
}

impl WakeWordModel {
    pub fn new(models: &[impl AsRef<Path>]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut wakeword = Self {
            mel_model: MelspectrogramModel::new()?,
            emb_model: EmbeddingModel::new()?,
            classifiers: HashMap::new(),
        };

        for path in models {
            wakeword.load_model(path, None)?;
        }

        Ok(wakeword)
    }

    /// Load a wake word classifier ONNX model from disk.
    ///
    /// If `model_name` is `None`, the file stem is used as the classifier name.
    pub fn load_model(
        &mut self,
        model_path: impl AsRef<Path>,
        model_name: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = model_path.as_ref();
        if !path.exists() {
            return Err(format!("Wake word model not found: {}", path.display()).into());
        }

        let name = match model_name {
            Some(n) => n.to_string(),
            None => path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string(),
        };

        let session = build_session_from_file(path)?;
        self.classifiers.insert(name, session);
        Ok(())
    }

    /// Get wake word predictions for an audio chunk.
    ///
    /// Pass ~2 seconds of 16 kHz i16 PCM audio. Shorter chunks that produce
    /// fewer than [`MIN_EMBEDDINGS`] embeddings return zero scores.
    pub fn predict(
        &mut self,
        audio_chunk: &Array1<i16>,
    ) -> Result<HashMap<String, f32>, Box<dyn std::error::Error>> {
        if self.classifiers.is_empty() {
            return Ok(HashMap::new());
        }

        // Mel spectrogram over the full chunk
        let mel = self.mel_model.detect(audio_chunk)?;
        let num_frames = mel.shape()[0];

        if num_frames < EMBEDDING_WINDOW {
            return Ok(self.zero_scores());
        }

        // Extract embeddings: 76-frame windows, stride 8
        let mut embeddings = Vec::new();
        let mut start = 0;
        while start + EMBEDDING_WINDOW <= num_frames {
            let window = mel.slice(ndarray::s![start..start + EMBEDDING_WINDOW, ..]).to_owned();
            let emb = self.emb_model.detect(&window)?;
            embeddings.push(emb);
            start += EMBEDDING_STRIDE;
        }

        if embeddings.len() < MIN_EMBEDDINGS {
            return Ok(self.zero_scores());
        }

        // Use last MIN_EMBEDDINGS embeddings -> shape (1, 16, 96)
        let last = &embeddings[embeddings.len() - MIN_EMBEDDINGS..];
        let views: Vec<_> = last.iter().map(|e| e.view()).collect();
        let emb_sequence = ndarray::stack(Axis(0), &views)?;
        let emb_input = emb_sequence.insert_axis(Axis(0));

        // Run each classifier
        let mut predictions = HashMap::new();
        for (name, session) in &mut self.classifiers {
            let tensor = Tensor::from_array(emb_input.clone())?;
            let outputs = session.run(ort::inputs!["embeddings" => tensor])?;
            let raw = outputs["score"].try_extract_array::<f32>()?;
            let score = raw.iter().copied().next().unwrap_or(0.0);
            predictions.insert(name.clone(), score);
        }

        Ok(predictions)
    }

    fn zero_scores(&self) -> HashMap<String, f32> {
        self.classifiers.keys().map(|k| (k.clone(), 0.0)).collect()
    }
}
