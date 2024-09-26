#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]

include!("soxr.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        unsafe {
            let version = soxr_version();
            let version = std::ffi::CStr::from_ptr(version).to_str().unwrap();
            println!("version: {}", version);
        }
    }

    #[test]
    fn test_stream() {
        use std::ffi::c_void;
        use std::ffi::CStr;
        use std::ptr;

        use hound::{WavReader, WavSpec, WavWriter};

        let input_wav_path = "input.wav";
        let output_wav_path = "output.wav";

        let mut reader = WavReader::open(input_wav_path).expect("Failed to open input WAV file");

        let wav_spec = reader.spec();
        let input_rate = wav_spec.sample_rate as f64;
        let output_rate = 24000.0;

        let num_channels = wav_spec.channels as u32;

        let samples: Vec<i16> =
            reader.samples::<i16>().map(|s| s.expect("Failed to read sample")).collect();

        let buf_total_len = samples.len();
        let olen =
            ((output_rate * buf_total_len as f64) / (input_rate + output_rate) + 0.5) as usize;
        let ilen = buf_total_len - olen;

        let mut obuf = vec![0i16; olen];

        let mut odone: usize = 0;
        let mut need_input = true;

        let mut error: soxr_error_t = ptr::null();

        let io_spec = soxr_io_spec {
            itype: SOXR_INT16_I as u32,
            otype: SOXR_INT16_I as u32,
            scale: 1.0,
            e: ptr::null_mut(),
            flags: 0,
        };

        let soxr = unsafe {
            soxr_create(
                input_rate,
                output_rate,
                num_channels,
                &mut error,
                &io_spec,
                ptr::null(),
                ptr::null(),
            )
        };

        if error.is_null() {
            let mut input_pos = 0;
            let input_len = samples.len();

            let mut output_samples = Vec::new();

            while error.is_null() && (need_input || odone > 0) {
                let mut ilen1 = 0;
                let mut ibuf: Option<&[i16]> = None;

                if need_input {
                    if input_pos < input_len {
                        let remaining_samples = input_len - input_pos;
                        let samples_to_read = std::cmp::min(ilen, remaining_samples);

                        ibuf = Some(&samples[input_pos..input_pos + samples_to_read]);
                        ilen1 = samples_to_read;
                        input_pos += samples_to_read;
                    } else {
                        ibuf = None;
                    }
                }

                let in_ptr = match ibuf {
                    Some(slice) => slice.as_ptr() as *const c_void,
                    None => ptr::null(),
                };

                let process_error = unsafe {
                    soxr_process(
                        soxr,
                        in_ptr,
                        ilen1,
                        ptr::null_mut(),
                        obuf.as_mut_ptr() as *mut c_void,
                        olen,
                        &mut odone,
                    )
                };

                if !process_error.is_null() {
                    break;
                }

                if odone > 0 {
                    output_samples.extend_from_slice(&obuf[..odone]);
                }

                need_input = (odone < olen) && ibuf.is_some();
            }

            let spec = WavSpec {
                channels: wav_spec.channels,
                sample_rate: output_rate as u32,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };

            let mut writer =
                WavWriter::create(output_wav_path, spec).expect("Failed to create output WAV file");

            for sample in output_samples {
                writer.write_sample(sample).expect("Failed to write sample");
            }

            writer.finalize().expect("Failed to finalize WAV file");

            println!("Resampling completed successfully.");
        } else {
            let error_str = unsafe { CStr::from_ptr(error) };
            eprintln!("Error creating resampler: {}", error_str.to_string_lossy());
        }

        unsafe {
            soxr_delete(soxr);
        }
    }
}
