use std::{f32, f64};

// Implementation based on https://chromium.googlesource.com/chromium/src.git/+/refs/heads/main/media/base/sinc_resampler.cc

#[derive(Debug)]
pub struct SincResampler {
    kernel_storage: Box<[f32]>,
    kernel_pre_sinc_storage: Box<[f32]>,
    kernel_window_storage: Box<[f32]>,
    input_buffer: Box<[f32]>,
    kernel_size: usize,
    io_sample_rate_ratio: f64,
    request_frames: usize,
}

impl SincResampler {
    const MAX_KERNEL_SIZE: usize = 64;
    const MIN_KERNEL_SIZE: usize = 32;
    const DEFAULT_REQUEST_SIZE: usize = 512;
    const SMALL_REQUEST_SIZE: usize = Self::MAX_KERNEL_SIZE * 2;
    const KERNEL_OFFSET_COUNT: usize = 32;

    // Blackman window parameters
    // See https://en.wikipedia.org/wiki/Window_function
    const A0: f64 = 0.42;
    const A1: f64 = 0.5;
    const A2: f64 = 0.08;

    pub fn new(io_sample_rate_ratio: f64, request_frames: usize) -> Self {
        let kernel_size = kernel_size_from_request_frames(request_frames);
        let kernel_storage_size = kernel_size * (Self::KERNEL_OFFSET_COUNT + 1);
        let input_buffer_size = request_frames + kernel_size;

        // Initialize kernel
        let mut kernel_storage = vec![0.0; kernel_storage_size].into_boxed_slice();
        let mut kernel_pre_sinc_storage = vec![0.0; kernel_storage_size].into_boxed_slice();
        let mut kernel_window_storage = vec![0.0; kernel_storage_size].into_boxed_slice();
        let input_buffer = vec![0.0; input_buffer_size].into_boxed_slice();

        let sinc_scale_factor = sinc_scale_factor(io_sample_rate_ratio, kernel_size);
        for offset_idx in 0..Self::KERNEL_OFFSET_COUNT + 1 {
            let subsample_offset = offset_idx as f32 / Self::KERNEL_OFFSET_COUNT as f32;
            for i in 0..kernel_size {
                let idx = i + offset_idx * kernel_size;
                let pre_sinc =
                    f32::consts::PI * (i as f32 - kernel_size as f32 / 2.0 - subsample_offset);

                kernel_pre_sinc_storage[idx] = pre_sinc;

                // Blackman window
                let x = (i as f64 - subsample_offset as f64) / kernel_size as f64;
                let window = (Self::A0 - Self::A1 * (2.0 * f64::consts::PI * x).cos()
                    + Self::A2 * (4.0 * f64::consts::PI * x).cos())
                    as f32;
                kernel_window_storage[idx] = window;

                // Compute the sinc with offset and the window
                let a = if pre_sinc != 0.0 {
                    (sinc_scale_factor as f32 * pre_sinc).sin() / pre_sinc
                } else {
                    sinc_scale_factor as f32
                };
                kernel_storage[idx] = a;
            }
        }

        Self {
            kernel_storage,
            kernel_pre_sinc_storage,
            kernel_window_storage,
            input_buffer,
            kernel_size,
            io_sample_rate_ratio,
            request_frames,
        }
    }

    pub fn update_regions(&mut self, second_load: bool) {
        self.r0 = if second_load { self.kernel_size } else { self.kernel_size / 2 };
        self.r3 = self.r0 + self.request_frames - self.kernel_size;
        self.r4 = self.r0 + self.request_frames - self.kernel_size / 2;

        self.block_size = self.r4 - self.r2;
        self.chunk_size = calculate_chunk_size(self.block_size, self.io_sample_rate_ratio);
    }

    pub fn resample(&mut self, dst: &mut [f32]) {
        let dst_len = (self.request_frames as f64 / self.io_sample_rate_ratio) as usize;

        for x in 0..dst_len {
            let virtual_index = x as f64 * self.io_sample_rate_ratio;
            let virtual_offset =
                (virtual_index - virtual_index.floor()) * Self::KERNEL_OFFSET_COUNT as f64;

            let offset = virtual_offset as usize; // subsample kernel index

            let k1 = offset * self.kernel_size;
            let k2 = k1 + self.kernel_size; // End of the subsample kernel

            let input_index = virtual_index as usize;
            let kernel_interpolation_factor = virtual_offset - offset as f64;

            dst[x] = convolve(
                self.kernel_size,
                &self.input_buffer[input_index..],
                &self.kernel_storage[k1..k2],
                &self.kernel_storage[k2..k2 + self.kernel_size],
                kernel_interpolation_factor,
            );
        }
    }

    pub fn set_ratio(&mut self, io_sample_rate_ratio: f64) {
        if (self.io_sample_rate_ratio - io_sample_rate_ratio).abs() < f64::EPSILON {
            return;
        }

        self.io_sample_rate_ratio = io_sample_rate_ratio;
        let sinc_scale_factor = sinc_scale_factor(io_sample_rate_ratio, self.kernel_size);
    }

    /*pub fn prime_with_silence(&mut self) {
        self.update_regions(true);
    }

    pub fn flush(&mut self) {
        self.buffer_primed = false;
        self.virtual_source_idx = 0.0;
        self.input_buffer.fill(0.0);
        self.update_regions(false);
    }

    pub fn max_input_frames_requested(&self, output_frames_requested: usize) -> usize {
        let num_chunks: usize =
            (output_frames_requested as f32 / self.chunk_size as f32).ceil() as usize;
        num_chunks * self.request_frames
    }*/

    pub fn kernel_size(&self) -> usize {
        self.kernel_size
    }
}

// TODO(theomonnom): SIMD implementation
fn convolve(
    kernel_size: usize,
    input: &[f32],
    k1: &[f32],
    k2: &[f32],
    kernel_interpolation_factor: f64,
) -> f32 {
    let mut sum1 = 0.0_f32;
    let mut sum2 = 0.0_f32;

    for i in 0..kernel_size {
        sum1 += input[i] * k1[i];
        sum2 += input[i] * k2[i];
    }

    ((1.0 - kernel_interpolation_factor) * sum1 as f64 + kernel_interpolation_factor * sum2 as f64)
        as f32
}

fn sinc_scale_factor(io_ratio: f64, kernel_size: usize) -> f64 {
    let mut sinc_scale_factor = if io_ratio > 1.0 { 1.0 / io_ratio } else { 1.0 };

    if kernel_size == SincResampler::MAX_KERNEL_SIZE {
        sinc_scale_factor *= 0.92;
    } else if kernel_size == SincResampler::MIN_KERNEL_SIZE {
        sinc_scale_factor *= 0.90;
    }

    sinc_scale_factor
}

fn kernel_size_from_request_frames(request_frames: usize) -> usize {
    const SMALL_KERNEL_LIMIT: usize = SincResampler::MAX_KERNEL_SIZE * 3 / 2;
    if request_frames <= SMALL_KERNEL_LIMIT {
        SincResampler::MIN_KERNEL_SIZE
    } else {
        SincResampler::MAX_KERNEL_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    // Write the kernel buffers into a CSV file for plotting.
    #[test]
    pub fn plot_kernels() {
        let mut file = File::create("plot_kernels.csv").unwrap();
        let resampler = SincResampler::new(1.0, SincResampler::DEFAULT_REQUEST_SIZE);

        writeln!(file, "x, kernel_pre_sinc_storage, kernel_window_storage, kernel_storage")
            .unwrap();

        for x in 0..resampler.kernel_storage.len() {
            // Convert each float to a string with commas instead of dots
            let kernel_pre_sinc_storage = resampler.kernel_pre_sinc_storage[x];
            let kernel_window_storage = resampler.kernel_window_storage[x];
            let kernel_storage = resampler.kernel_storage[x];

            writeln!(
                file,
                "\"{}\",\"{}\",\"{}\",\"{}\"",
                x,
                kernel_pre_sinc_storage.to_string().replace(".", ","),
                kernel_window_storage.to_string().replace(".", ","),
                kernel_storage.to_string().replace(".", ","),
            )
            .unwrap();
        }
    }
}
