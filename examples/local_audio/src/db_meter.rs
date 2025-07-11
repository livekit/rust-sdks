use anyhow::Result;

use std::io::{self, Write};
use tokio::sync::mpsc;

// DB meter related constants
pub const DB_METER_UPDATE_INTERVAL_MS: u64 = 50; // Update every 50ms
const DB_METER_WIDTH: usize = 40; // Width of the dB meter bar

/// Calculate decibel level from audio samples
pub fn calculate_db_level(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return -60.0; // Very quiet
    }
    
    // Calculate RMS
    let sum_squares: f64 = samples.iter()
        .map(|&sample| {
            let normalized = sample as f64 / i16::MAX as f64;
            normalized * normalized
        })
        .sum();
    
    let rms = (sum_squares / samples.len() as f64).sqrt();
    
    // Convert to dB (20 * log10(rms))
    if rms > 0.0 {
        20.0 * rms.log10() as f32
    } else {
        -60.0 // Very quiet
    }
}

/// Format the dB level as a visual meter
fn format_db_meter(db_level: f32) -> String {
    let db_clamped = db_level.clamp(-60.0, 0.0);
    let normalized = (db_clamped + 60.0) / 60.0; // Normalize to 0.0-1.0
    let filled_width = (normalized * DB_METER_WIDTH as f32) as usize;
    
    let mut meter = String::new();
    meter.push_str("\r"); // Return to start of line
    meter.push_str("Mic Level: ");
    
    // Add the dB value
    meter.push_str(&format!("{:>5.1} dB ", db_level));
    
    // Add the visual meter
    meter.push('[');
    for i in 0..DB_METER_WIDTH {
        if i < filled_width {
            if i < DB_METER_WIDTH * 2 / 3 {
                meter.push('█'); // Full block for low/medium levels
            } else if i < DB_METER_WIDTH * 9 / 10 {
                meter.push('▓'); // Medium block for high levels
            } else {
                meter.push('▒'); // Light block for very high levels (clipping warning)
            }
        } else {
            meter.push('░'); // Light shade for empty
        }
    }
    meter.push(']');

    meter
}

/// Display the dB meter continuously
pub async fn display_db_meter(mut db_rx: mpsc::UnboundedReceiver<f32>) -> Result<()> {
    let mut last_update = std::time::Instant::now();
    let mut current_db = -60.0f32;
    
    println!("\nLocal Audio Level");
    println!("────────────────────────────────────────");
    
    loop {
        tokio::select! {
            db_level = db_rx.recv() => {
                if let Some(db) = db_level {
                    current_db = db;
                    
                    // Update display at regular intervals
                    if last_update.elapsed().as_millis() >= DB_METER_UPDATE_INTERVAL_MS as u128 {
                        print!("{}", format_db_meter(current_db));
                        io::stdout().flush().unwrap();
                        last_update = std::time::Instant::now();
                    }
                } else {
                    break;
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(DB_METER_UPDATE_INTERVAL_MS)) => {
                // Update display even if no new data
                print!("{}", format_db_meter(current_db));
                io::stdout().flush().unwrap();
            }
        }
    }
    
    Ok(())
} 