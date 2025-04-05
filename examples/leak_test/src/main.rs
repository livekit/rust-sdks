use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::prelude::{AudioFrame, AudioSourceOptions, RtcAudioSource};
use std::collections::HashMap;
use std::io::Write;
use std::io::{self, BufRead};
use std::process::Command;
use std::time::Duration;

// Function to get the current process ID
fn get_pid() -> u32 {
    std::process::id()
}

// Function to get detailed thread information including thread names
fn get_detailed_thread_info() -> HashMap<String, i32> {
    let pid = get_pid();
    let mut thread_info = HashMap::new();

    // Get thread information from process tree (Linux specific)
    match Command::new("sh")
        .arg("-c")
        .arg(format!("ls -l /proc/{}/task/*/comm 2>/dev/null | while read line; do cat $line; done | sort | uniq -c", pid))
        .output()
    {
        Ok(output) => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                let parts: Vec<&str> = line.trim().split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(count) = parts[0].parse::<i32>() {
                        let name = parts[1..].join(" ");
                        thread_info.insert(name, count);
                    }
                }
            }
        },
        Err(e) => {
            println!("Error getting thread names: {}", e);
        }
    }

    // Get total thread count
    match Command::new("sh").arg("-c").arg(format!("ps -T -p {} | wc -l", pid)).output() {
        Ok(output) => {
            let count_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(count) = count_str.parse::<i32>() {
                thread_info.insert("TOTAL".to_string(), count - 1); // Subtract header row
            }
        }
        Err(e) => {
            println!("Failed to count threads: {}", e);
        }
    }

    thread_info
}

// Function to print thread information and return the total thread count
fn print_thread_info() -> i32 {
    let thread_info = get_detailed_thread_info();

    println!("=== Thread Information ===");
    println!("PID: {}", get_pid());

    let mut total = 0;
    for (name, count) in &thread_info {
        if name == "TOTAL" {
            total = *count;
            println!("Total Thread Count: {}", count);
        } else {
            println!("- {} Threads: {}", name, count);
        }
    }

    // Look specifically for audio-related threads
    match Command::new("sh")
        .arg("-c")
        .arg(format!("ps -T -p {} | grep -i audio", get_pid()))
        .output()
    {
        Ok(output) => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if !output_str.is_empty() {
                println!("\nAudio-related Threads:");
                println!("{}", output_str);
            }
        }
        Err(e) => {
            println!("Failed to find audio threads: {}", e);
        }
    }

    // Display memory usage
    match Command::new("sh")
        .arg("-c")
        .arg(format!("ps -o pid,rss -p {} | tail -n 1", get_pid()))
        .output()
    {
        Ok(output) => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let parts: Vec<&str> = output_str.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(rss) = parts[1].parse::<i32>() {
                    println!("\nMemory Usage: {} KB", rss);
                }
            }
        }
        Err(e) => {
            println!("Failed to get memory usage: {}", e);
        }
    }

    println!("==================");
    total
}

#[tokio::main]
async fn main() -> io::Result<()> {
    println!("=== AudioSourceCapture Leak Test ===");

    // Record the initial state
    println!("\n[Before Test]");
    let initial_thread_count = print_thread_info();

    // Menu for test selection
    println!("\nSelect test mode:");
    println!("1. Short-term Test (5 iterations)");
    println!("2. Long-term Load Test (100 iterations)");
    println!("3. Memory Monitoring Test (Continue until Enter key is pressed)");
    println!("4. Multiple Source Test (Hold multiple AudioSources simultaneously)");
    println!("5. Memory Leak Focus Test (Repeat creation and release)");
    println!("6. Thread Creation Verification Test (Verify if AudioSourceCapture creates threads)");
    print!("Select (1-6): ");
    io::stdout().flush().unwrap();

    let stdin = io::stdin();
    let mut input = String::new();
    stdin.lock().read_line(&mut input)?;

    let choice = input.trim().parse::<u32>().unwrap_or(1);

    match choice {
        1 => run_short_test(initial_thread_count).await,
        2 => run_long_test(initial_thread_count).await,
        3 => run_continuous_test(initial_thread_count).await,
        4 => run_multiple_sources_test(initial_thread_count).await,
        5 => run_memory_leak_test(initial_thread_count).await,
        6 => run_thread_creation_test().await,
        _ => run_short_test(initial_thread_count).await,
    }

    Ok(())
}

// Short-term test (5 iterations)
async fn run_short_test(initial_thread_count: i32) {
    let iterations = 5;
    println!("\n[Short-term Test Started] - {} iterations", iterations);

    for i in 1..=iterations {
        println!("\n=== Iteration {} / {} ===", i, iterations);
        test_audio_source_leak(5).await;

        println!("Waiting 3 seconds after releasing AudioSource...");
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Get current thread count
        let current_thread_count = print_thread_info();
        let diff = current_thread_count - initial_thread_count;

        if diff > 0 {
            println!("⚠️ Thread increase detected: +{}", diff);
        } else {
            println!("✓ Thread count normal");
        }
    }

    print_test_result(initial_thread_count);
}

// Long-term load test (100 iterations)
async fn run_long_test(initial_thread_count: i32) {
    let iterations = 100;
    println!("\n[Long-term Load Test Started] - {} iterations", iterations);

    for i in 1..=iterations {
        if i % 10 == 0 || i == 1 {
            println!("\n=== Iteration {} / {} ===", i, iterations);
        }

        test_audio_source_leak(5).await;

        // Output thread information every 10 iterations
        if i % 10 == 0 || i == iterations {
            println!("Checking thread state...");
            let current_thread_count = print_thread_info();
            let diff = current_thread_count - initial_thread_count;

            if diff > 0 {
                println!("⚠️ Thread increase detected: +{}", diff);
            } else {
                println!("✓ Thread count normal");
            }
        }

        // Sleep briefly to reduce load
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    print_test_result(initial_thread_count);
}

// Memory monitoring test (continues until Enter key is pressed)
async fn run_continuous_test(initial_thread_count: i32) {
    println!("\n[Memory Monitoring Test Started] - Continue until Enter key is pressed");
    println!("You can run 'top -p {}' in another terminal to monitor memory usage", get_pid());

    // Task for waiting for input
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
    let input_task = tokio::spawn(async move {
        let stdin = io::stdin();
        let mut input = String::new();
        stdin.read_line(&mut input).unwrap();
        let _ = tx.send(()).await;
    });

    let mut i = 0;
    loop {
        i += 1;

        // Display status every 10 iterations
        if i % 10 == 0 {
            println!("\n=== Iteration {} ===", i);
        }

        test_audio_source_leak(3).await;

        // Check for exit signal
        if rx.try_recv().is_ok() {
            break;
        }

        // Output thread information every 50 iterations
        if i % 50 == 0 {
            println!("Checking thread state...");
            let current_thread_count = print_thread_info();
            let diff = current_thread_count - initial_thread_count;

            if diff > 0 {
                println!("⚠️ Thread increase detected: +{}", diff);
            }
        }

        // Sleep briefly to reduce load
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Cancel the input waiting task
    input_task.abort();

    print_test_result(initial_thread_count);
}

// Output test results
fn print_test_result(initial_thread_count: i32) {
    println!("\n[Test Completed]");
    let final_thread_count = print_thread_info();
    let diff = final_thread_count - initial_thread_count;

    println!("\n=== Results ===");
    println!("Initial Thread Count: {}", initial_thread_count);
    println!("Final Thread Count: {}", final_thread_count);
    println!("Change: {:+}", diff);

    if diff > 0 {
        println!("\n⚠️ Leak Detected!");
        println!("Thread count increased by {}. AudioSourceCapture threads may not be terminating properly.", diff);
    } else {
        println!("\n✓ No Leak");
        println!("Thread count didn't increase. The fix appears to be working.");
    }

    println!("\nRecommended Fix:");
    println!("Modify the InternalSource destructor in webrtc-sys/src/audio_track.cpp as follows:");
    println!("```cpp");
    println!("AudioTrackSource::InternalSource::~InternalSource() {{");
    println!("  // Stop the repeating task first");
    println!("  if (queue_size_samples_) {{");
    println!("    audio_task_.Stop();");
    println!("    audio_queue_ = nullptr;");
    println!("  }}");
    println!("  delete[] silence_buffer_;");
    println!("}}");
    println!("```");
}

// Test for holding multiple AudioSources simultaneously
async fn run_multiple_sources_test(initial_thread_count: i32) {
    println!("\n[Multiple Source Test Started]");

    // Create and hold multiple AudioSources simultaneously
    let count = 20; // Number of AudioSources to hold simultaneously
    println!("Creating and holding {} AudioSources simultaneously", count);

    let mut sources = Vec::with_capacity(count);
    let samples = vec![0i16; 960]; // 10ms of 48kHz stereo audio

    for i in 1..=count {
        println!("Creating AudioSource {} / {}...", i, count);

        let source = NativeAudioSource::new(
            AudioSourceOptions::default(),
            48000, // Sample rate
            2,     // Channels
            1000,  // Queue size (ms)
        );

        // Capture one frame for each source
        let audio_frame = AudioFrame {
            data: samples.as_slice().into(),
            sample_rate: 48000,
            num_channels: 2,
            samples_per_channel: 480,
        };

        source.capture_frame(&audio_frame).await.unwrap();

        // Store in vector to maintain reference
        sources.push(source);
    }

    println!("\nAll AudioSources created. Current thread state:");
    print_thread_info();

    println!("\nWaiting 10 seconds...");
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Release one by one
    println!("\nReleasing AudioSources one by one");
    for i in 0..sources.len() {
        println!("Releasing AudioSource {} / {}...", i + 1, count);
        sources.pop();

        // Check thread state every 5 releases
        if (i + 1) % 5 == 0 || i == sources.len() - 1 {
            println!("\nThread state after releasing {} AudioSources:", i + 1);
            let current_count = print_thread_info();
            let diff = current_count - initial_thread_count;

            if diff > 0 {
                println!("⚠️ Thread increase detected: +{}", diff);
            } else {
                println!("✓ Thread count normal");
            }
        }

        // Wait briefly between releases
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // Wait a bit to check final state
    println!("\nAll AudioSources released. Waiting 5 seconds...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    print_test_result(initial_thread_count);
}

// Memory leak focus test
async fn run_memory_leak_test(initial_thread_count: i32) {
    println!("\n[Memory Leak Focus Test Started]");

    // Periodically check memory usage
    let iterations = 20;
    let sources_per_iteration = 5;

    for i in 1..=iterations {
        println!("\n=== Iteration {} / {} ===", i, iterations);

        // First create multiple AudioSources
        let mut sources = Vec::with_capacity(sources_per_iteration);
        let samples = vec![0i16; 960];

        for _j in 1..=sources_per_iteration {
            let source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 2, 1000);

            // Capture multiple frames to apply heavy load
            for _k in 1..=5 {
                let audio_frame = AudioFrame {
                    data: samples.as_slice().into(),
                    sample_rate: 48000,
                    num_channels: 2,
                    samples_per_channel: 480,
                };

                source.capture_frame(&audio_frame).await.unwrap();
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            sources.push(source);
        }

        // Check current state
        println!("Thread and memory state while holding {} AudioSources:", sources.len());
        print_thread_info();

        // Hold for 2 seconds
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Release all AudioSources
        println!("Releasing all AudioSources...");
        sources.clear(); // Explicitly release

        // Check state after release
        println!("Thread and memory state after release:");
        let current_count = print_thread_info();
        let diff = current_count - initial_thread_count;

        if diff > 0 {
            println!("⚠️ Warning: Thread count increased +{}", diff);
        } else {
            println!("✓ Thread count normal");
        }

        // Brief wait to promote GC
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // Wait to check final state
    println!("\nAll tests completed. Waiting 5 seconds to check memory release...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    print_test_result(initial_thread_count);
}

// Test to verify if AudioSourceCapture creates threads under specific conditions
async fn run_thread_creation_test() {
    println!("\n[AudioSourceCapture Thread Creation Test]");

    // Record thread state before test
    println!("Thread state before test:");
    let initial_thread_count = print_thread_info();

    println!("\n1. Creating AudioSource with queue_size=0 (shouldn't create threads)");
    let source_without_queue = NativeAudioSource::new(
        AudioSourceOptions::default(),
        48000,
        2,
        0, // Set queue size to 0 - shouldn't create threads
    );

    // Wait a bit
    tokio::time::sleep(Duration::from_secs(1)).await;

    println!("Thread state after creating AudioSource with queue_size=0:");
    let thread_count_without_queue = print_thread_info();
    let diff1 = thread_count_without_queue - initial_thread_count;

    if diff1 == 0 {
        println!("✓ As expected: Thread count didn't change with queue_size=0");
    } else {
        println!("⚠️ Unexpected: Thread count changed with queue_size=0: {:+}", diff1);
    }

    // Release source
    drop(source_without_queue);
    tokio::time::sleep(Duration::from_secs(1)).await;

    println!("\n2. Creating AudioSource with queue_size=1000 (should create threads)");
    let source_with_queue = NativeAudioSource::new(
        AudioSourceOptions::default(),
        48000,
        2,
        1000, // queue_size>0 - should create threads
    );

    // Wait a bit to give time for thread creation
    tokio::time::sleep(Duration::from_secs(1)).await;

    println!("Thread state after creating AudioSource with queue_size=1000:");
    let thread_count_with_queue = print_thread_info();
    let diff2 = thread_count_with_queue - initial_thread_count;

    if diff2 > 0 {
        println!("✓ As expected: Thread count increased with queue_size=1000: {:+}", diff2);
        println!("  This indicates that AudioSourceCapture threads were created");
    } else {
        println!("⚠️ Unexpected: Thread count didn't change with queue_size=1000");
        println!("  AudioSourceCapture threads might not be created");
        println!("  This could be due to implementation changes or delayed thread creation");
    }

    // Send sample data
    let samples = vec![0i16; 960];
    let audio_frame = AudioFrame {
        data: samples.as_slice().into(),
        sample_rate: 48000,
        num_channels: 2,
        samples_per_channel: 480,
    };

    source_with_queue.capture_frame(&audio_frame).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    println!("\nThread state after frame capture:");
    let thread_count_after_capture = print_thread_info();
    let diff3 = thread_count_after_capture - initial_thread_count;

    if diff3 > 0 {
        println!("✓ Thread count increase maintained after frame capture: {:+}", diff3);
    } else {
        println!("⚠️ Thread count returned to initial value after frame capture");
    }

    // Release source
    println!("\n3. Releasing AudioSource");
    drop(source_with_queue);

    // Wait after release
    tokio::time::sleep(Duration::from_secs(3)).await;

    println!("Thread state after AudioSource release:");
    let final_thread_count = print_thread_info();
    let diff4 = final_thread_count - initial_thread_count;

    if diff4 == 0 {
        println!("✓ Normal: Thread count returned to initial value after AudioSource release");
        println!("  This indicates that AudioSourceCapture threads were properly terminated");
    } else {
        println!("⚠️ Warning: Thread count change persists after AudioSource release: {:+}", diff4);
        println!("  This suggests that AudioSourceCapture threads might be leaking");
    }

    println!("\n[Conclusion]");
    if diff2 > 0 && diff4 == 0 {
        println!("✓ Test succeeded: AudioSourceCapture creates threads and properly releases them");
    } else if diff2 == 0 {
        println!("⚠️ Test inconclusive: Couldn't verify if AudioSourceCapture creates threads");
    } else {
        println!("⚠️ Test failed: AudioSourceCapture threads might not be properly released");
    }
}

async fn test_audio_source_leak(capture_count: i32) {
    println!("Creating AudioSource...");

    // Create audio source
    let source = NativeAudioSource::new(
        AudioSourceOptions::default(),
        48000, // Sample rate
        2,     // Channels
        1000,  // Queue size (ms)
    );

    // Send sample data
    let samples = vec![0i16; 960]; // 10ms of 48kHz stereo audio

    for _i in 1..=capture_count {
        let audio_frame = AudioFrame {
            data: samples.as_slice().into(),
            sample_rate: 48000,
            num_channels: 2,
            samples_per_channel: 480,
        };

        // Capture frame
        source.capture_frame(&audio_frame).await.unwrap();

        // Wait a bit between captures
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    println!("Releasing AudioSource");
    // Source should be released here
}
