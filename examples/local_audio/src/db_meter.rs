use anyhow::Result;

use std::io::{self, Write};
use tokio::sync::mpsc;

// DB meter related constants
pub const DB_METER_UPDATE_INTERVAL_MS: u64 = 50; // Update every 50ms
const MIC_METER_WIDTH: usize = 25; // Width of the mic dB meter bar
const ROOM_METER_WIDTH: usize = 25; // Width of the room dB meter bar (reduced from 35)

// ANSI color codes for colorful meters
const COLOR_RESET: &str = "\x1b[0m";
const COLOR_GREEN: &str = "\x1b[32m";
const COLOR_YELLOW: &str = "\x1b[33m";
const COLOR_RED: &str = "\x1b[31m";
const COLOR_BRIGHT_GREEN: &str = "\x1b[92m";
const COLOR_BRIGHT_YELLOW: &str = "\x1b[93m";
const COLOR_BRIGHT_RED: &str = "\x1b[91m";
const COLOR_DIM: &str = "\x1b[2m";

/// Calculate decibel level from audio samples
pub fn calculate_db_level(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return -60.0; // Very quiet
    }

    // Calculate RMS
    let sum_squares: f64 = samples
        .iter()
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

/// Get color based on dB level and position in meter
fn get_meter_color(db_level: f32, position_ratio: f32) -> &'static str {
    // Determine color based on both dB level and position in the meter
    if db_level > -6.0 && position_ratio > 0.85 {
        COLOR_BRIGHT_RED // Clipping/very loud
    } else if db_level > -12.0 && position_ratio > 0.7 {
        COLOR_RED // Loud
    } else if db_level > -18.0 && position_ratio > 0.5 {
        COLOR_BRIGHT_YELLOW // Medium-loud
    } else if db_level > -30.0 && position_ratio > 0.3 {
        COLOR_YELLOW // Medium
    } else if position_ratio > 0.1 {
        COLOR_BRIGHT_GREEN // Low-medium
    } else {
        COLOR_GREEN // Low
    }
}

/// Format a single dB meter with colors
fn format_single_meter(db_level: f32, meter_width: usize, meter_label: &str) -> String {
    let db_clamped = db_level.clamp(-60.0, 0.0);
    let normalized = (db_clamped + 60.0) / 60.0; // Normalize to 0.0-1.0
    let filled_width = (normalized * meter_width as f32) as usize;

    let mut meter = String::new();
    meter.push_str(meter_label);

    // Add the dB value with appropriate color
    let db_color = if db_level > -6.0 {
        COLOR_BRIGHT_RED
    } else if db_level > -12.0 {
        COLOR_RED
    } else if db_level > -24.0 {
        COLOR_YELLOW
    } else {
        COLOR_GREEN
    };
    meter.push_str(&format!("{}{:>5.1} dB{} ", db_color, db_level, COLOR_RESET));

    // Add the visual meter with colors
    meter.push('[');
    for i in 0..meter_width {
        let position_ratio = i as f32 / meter_width as f32;

        if i < filled_width {
            let color = get_meter_color(db_level, position_ratio);
            meter.push_str(color);
            meter.push('█'); // Full block for active levels
            meter.push_str(COLOR_RESET);
        } else {
            meter.push_str(COLOR_DIM);
            meter.push('░'); // Light shade for empty
            meter.push_str(COLOR_RESET);
        }
    }
    meter.push(']');

    meter
}

/// Format both dB meters on the same line
fn format_dual_meters(mic_db: f32, room_db: f32) -> String {
    let mic_meter = format_single_meter(mic_db, MIC_METER_WIDTH, "Mic: ");
    let room_meter = format_single_meter(room_db, ROOM_METER_WIDTH, "  Room: ");

    format!("{}{}", mic_meter, room_meter)
}

/// Display dual dB meters continuously
pub async fn display_dual_db_meters(
    mut mic_db_rx: mpsc::UnboundedReceiver<f32>,
    mut room_db_rx: mpsc::UnboundedReceiver<f32>,
) -> Result<()> {
    let mut last_update = std::time::Instant::now();
    let mut current_mic_db = -60.0f32;
    let mut current_room_db = -60.0f32;
    let mut first_display = true;

    loop {
        tokio::select! {
            db_level = mic_db_rx.recv() => {
                if let Some(db) = db_level {
                    current_mic_db = db;

                    // Update display at regular intervals
                    if last_update.elapsed().as_millis() >= DB_METER_UPDATE_INTERVAL_MS as u128 {
                        display_meters(current_mic_db, current_room_db, &mut first_display);
                        last_update = std::time::Instant::now();
                    }
                } else {
                    break;
                }
            }
            db_level = room_db_rx.recv() => {
                if let Some(db) = db_level {
                    current_room_db = db;

                    // Update display at regular intervals
                    if last_update.elapsed().as_millis() >= DB_METER_UPDATE_INTERVAL_MS as u128 {
                        display_meters(current_mic_db, current_room_db, &mut first_display);
                        last_update = std::time::Instant::now();
                    }
                } else {
                    // Room meter channel closed, continue with mic only
                    current_room_db = -60.0;
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(DB_METER_UPDATE_INTERVAL_MS)) => {
                // Update display even if no new data
                display_meters(current_mic_db, current_room_db, &mut first_display);
            }
        }
    }

    Ok(())
}

/// Display the meters with proper terminal control (no jumping)
fn display_meters(mic_db: f32, room_db: f32, first_display: &mut bool) {
    if *first_display {
        // Don't clear screen - just show header where we are
        println!();
        println!("{}Audio Levels Monitor{}", COLOR_BRIGHT_GREEN, COLOR_RESET);
        println!(
            "{}────────────────────────────────────────────────────────────────────────────────{}",
            COLOR_DIM, COLOR_RESET
        );
        *first_display = false;
    }

    // Clear current line and display meters in place
    print!("\r\x1B[K"); // Clear current line
    print!("{}", format_dual_meters(mic_db, room_db));
    io::stdout().flush().unwrap();
}

/// Display the dB meter continuously (legacy single meter function for compatibility)
pub async fn display_db_meter(mut db_rx: mpsc::UnboundedReceiver<f32>) -> Result<()> {
    let mut last_update = std::time::Instant::now();
    let mut current_db = -60.0f32;
    let mut first_display = true;

    loop {
        tokio::select! {
            db_level = db_rx.recv() => {
                if let Some(db) = db_level {
                    current_db = db;

                    // Update display at regular intervals
                    if last_update.elapsed().as_millis() >= DB_METER_UPDATE_INTERVAL_MS as u128 {
                        display_single_meter(current_db, &mut first_display);
                        last_update = std::time::Instant::now();
                    }
                } else {
                    break;
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(DB_METER_UPDATE_INTERVAL_MS)) => {
                // Update display even if no new data
                display_single_meter(current_db, &mut first_display);
            }
        }
    }

    Ok(())
}

/// Display a single meter with proper terminal control (no jumping)
fn display_single_meter(db_level: f32, first_display: &mut bool) {
    if *first_display {
        // Don't clear screen - just show header where we are
        println!();
        println!("{}Local Audio Level{}", COLOR_BRIGHT_GREEN, COLOR_RESET);
        println!("{}────────────────────────────────────────{}", COLOR_DIM, COLOR_RESET);
        *first_display = false;
    }

    // Clear current line and display meter in place
    print!("\r\x1B[K"); // Clear current line
    print!("{}", format_single_meter(db_level, 40, "Mic Level: "));
    io::stdout().flush().unwrap();
}
