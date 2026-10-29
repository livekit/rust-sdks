// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use ndarray::{Array2, ArrayD, IxDyn};
use ort::session::{Session, SessionInputValue};
use ort::tensor::TensorElementType;
use ort::value::{DynValue, Tensor, ValueType};

use crate::{WakeWordError, FRAME_SAMPLES};

const CHUNK_INPUT: &str = "chunk";
const SCORE_OUTPUT: &str = "score";
// 2 s of silence: the state every stream starts from, as in training.
const WARMUP_FRAMES: usize = 25;

// One streaming wake word model: inputs `chunk` (1, 1280) plus state tensors,
// outputs `score` plus `<state>_out`. The state starts at zero and is warmed up
// on silence; `reset` returns to that warm state.
pub(crate) struct StreamingClassifier {
    session: Session,
    // Graph input index of `chunk`; every other input is state.
    chunk_index: usize,
    // `<state>_out` for each state input, in graph input order.
    state_outputs: Vec<String>,
    state: Vec<DynValue>,
    warm: Vec<DynValue>,
    pending: Vec<f32>,
}

impl StreamingClassifier {
    pub fn new(session: Session, path: &str) -> Result<Self, WakeWordError> {
        let not_streaming = || WakeWordError::NotStreamingModel(path.to_string());

        let inputs = session.inputs();
        let chunk_index =
            inputs.iter().position(|i| i.name() == CHUNK_INPUT).ok_or_else(not_streaming)?;

        let mut state_outputs = Vec::with_capacity(inputs.len() - 1);
        let mut state = Vec::with_capacity(inputs.len() - 1);
        for input in inputs.iter().filter(|i| i.name() != CHUNK_INPUT) {
            let ValueType::Tensor { ty, shape, .. } = input.dtype() else {
                return Err(not_streaming());
            };
            let Ok(dims) = shape.iter().map(|&d| usize::try_from(d)).collect::<Result<Vec<_>, _>>()
            else {
                // A symbolic dimension: state must have a fixed shape.
                return Err(not_streaming());
            };
            state.push(zeros(*ty, &dims).ok_or_else(not_streaming)?);
            state_outputs.push(format!("{}_out", input.name()));
        }

        let mut classifier = Self {
            session,
            chunk_index,
            state_outputs,
            state,
            warm: Vec::new(),
            pending: Vec::new(),
        };
        let silence = [0.0; FRAME_SAMPLES];
        for _ in 0..WARMUP_FRAMES {
            classifier.step(&silence)?;
        }
        classifier.warm = classifier.state.iter().map(copy).collect::<Result<_, _>>()?;
        Ok(classifier)
    }

    // Forget all audio heard so far.
    pub fn reset(&mut self) -> Result<(), WakeWordError> {
        self.state = self.warm.iter().map(copy).collect::<Result<_, _>>()?;
        self.pending.clear();
        Ok(())
    }

    // Feed 16 kHz audio; returns the highest score of the 80 ms steps it completed,
    // if any. Leftover samples are kept for the next call.
    pub fn feed(&mut self, audio: &[f32]) -> Result<Option<f32>, WakeWordError> {
        self.pending.extend_from_slice(audio);
        let mut best: Option<f32> = None;
        let mut start = 0;
        while start + FRAME_SAMPLES <= self.pending.len() {
            let frame: [f32; FRAME_SAMPLES] = self.pending[start..start + FRAME_SAMPLES]
                .try_into()
                .expect("Slice is exactly one frame");
            let score = self.step(&frame)?;
            best = Some(best.map_or(score, |b| b.max(score)));
            start += FRAME_SAMPLES;
        }
        self.pending.drain(..start);
        Ok(best)
    }

    fn step(&mut self, frame: &[f32; FRAME_SAMPLES]) -> Result<f32, WakeWordError> {
        let chunk =
            Tensor::from_array(Array2::from_shape_vec((1, FRAME_SAMPLES), frame.to_vec())?)?;
        let mut chunk = Some(SessionInputValue::from(chunk));
        let mut state = self.state.iter();
        let inputs: Vec<SessionInputValue> = (0..=self.state.len())
            .map(|i| {
                if i == self.chunk_index {
                    chunk.take().expect("Single chunk input")
                } else {
                    SessionInputValue::from(state.next().expect("Input per state tensor"))
                }
            })
            .collect();

        let mut outputs = self.session.run(inputs.as_slice())?;
        let score = outputs
            .get(SCORE_OUTPUT)
            .ok_or_else(|| WakeWordError::ModelOutputMissing(SCORE_OUTPUT.to_string()))?
            .try_extract_array::<f32>()?
            .iter()
            .copied()
            .next()
            .unwrap_or(0.0);
        let next = self
            .state_outputs
            .iter()
            .map(|name| {
                outputs.remove(name).ok_or_else(|| WakeWordError::ModelOutputMissing(name.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        drop(outputs);
        self.state = next;
        Ok(score)
    }
}

fn zeros(ty: TensorElementType, dims: &[usize]) -> Option<DynValue> {
    let value = match ty {
        TensorElementType::Float32 => {
            Tensor::from_array(ArrayD::<f32>::zeros(IxDyn(dims))).ok()?.into_dyn()
        }
        TensorElementType::Int64 => {
            Tensor::from_array(ArrayD::<i64>::zeros(IxDyn(dims))).ok()?.into_dyn()
        }
        _ => return None,
    };
    Some(value)
}

fn copy(value: &DynValue) -> Result<DynValue, WakeWordError> {
    if let Ok(array) = value.try_extract_array::<f32>() {
        return Ok(Tensor::from_array(array.into_owned())?.into_dyn());
    }
    Ok(Tensor::from_array(value.try_extract_array::<i64>()?.into_owned())?.into_dyn())
}
