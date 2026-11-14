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

//! Switching the recording or playout device while audio may be active.

use super::{AudioError, AudioResult};

/// The steps of switching one direction (recording or playout) of the ADM,
/// kept apart from the runtime so every failure path can be tested.
pub(super) trait DeviceSwitchSteps {
    /// "recording" or "playout", for error and log messages.
    fn direction(&self) -> &'static str;
    fn is_initialized(&self) -> bool;
    /// The current selection, as a token [`restore`](Self::restore) accepts.
    fn current_selection(&self) -> String;
    fn stop(&self) -> bool;
    /// Selects a device by GUID or `index:<n>` ID.
    fn select(&self, device_id: &str) -> bool;
    fn restore(&self, selection: &str) -> bool;
    fn init(&self) -> bool;
    fn start(&self) -> bool;
}

/// Selects `device_id`, restarting audio on it if audio was active.
///
/// If any step after stopping fails, the partially opened device is closed and
/// audio resumes on the previously selected device, or on the platform default
/// if that device is gone, so a failed switch doesn't leave active audio stopped.
pub(super) fn switch_device(steps: &impl DeviceSwitchSteps, device_id: &str) -> AudioResult<()> {
    if !steps.is_initialized() {
        return if steps.select(device_id) { Ok(()) } else { Err(AudioError::DeviceNotFound) };
    }

    let previous_selection = steps.current_selection();
    if !steps.stop() {
        return Err(AudioError::OperationFailed(format!("stop_{} failed", steps.direction())));
    }

    let error = if !steps.select(device_id) {
        AudioError::DeviceNotFound
    } else if !steps.init() {
        AudioError::OperationFailed(format!("init_{} failed", steps.direction()))
    } else if !steps.start() {
        AudioError::OperationFailed(format!("start_{} failed", steps.direction()))
    } else {
        return Ok(());
    };

    resume_on_previous_device(steps, &previous_selection);
    Err(error)
}

fn resume_on_previous_device(steps: &impl DeviceSwitchSteps, previous_selection: &str) {
    // Close whatever the failed attempt left open before reopening the previous device
    steps.stop();
    if reopen(steps, previous_selection) {
        return;
    }
    // The previous device may be gone, for example unplugged; the platform default
    // (an empty selection) is the best remaining choice.
    if !previous_selection.is_empty() {
        steps.stop();
        if reopen(steps, "") {
            log::warn!(
                "PlatformAudio: resumed {} on the platform default after a failed switch",
                steps.direction()
            );
            return;
        }
    }
    log::warn!("PlatformAudio: failed to resume {} after a failed switch", steps.direction());
}

fn reopen(steps: &impl DeviceSwitchSteps, selection: &str) -> bool {
    steps.restore(selection) && steps.init() && steps.start()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// Records each step and fails the chosen calls, given as (step, nth call).
    struct FakeSteps {
        initialized: bool,
        failing_calls: Vec<(&'static str, usize)>,
        calls: RefCell<Vec<String>>,
    }

    impl FakeSteps {
        fn new(initialized: bool, failing_calls: &[(&'static str, usize)]) -> Self {
            Self {
                initialized,
                failing_calls: failing_calls.to_vec(),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn run(&self, step: &'static str, detail: &str) -> bool {
            let mut calls = self.calls.borrow_mut();
            let count = calls.iter().filter(|call| call.split(' ').next() == Some(step)).count();
            calls.push(if detail.is_empty() {
                step.to_string()
            } else {
                format!("{step} {detail}")
            });
            !self.failing_calls.contains(&(step, count + 1))
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl DeviceSwitchSteps for FakeSteps {
        fn direction(&self) -> &'static str {
            "recording"
        }

        fn is_initialized(&self) -> bool {
            self.initialized
        }

        fn current_selection(&self) -> String {
            "index:0".to_string()
        }

        fn stop(&self) -> bool {
            self.run("stop", "")
        }

        fn select(&self, device_id: &str) -> bool {
            self.run("select", device_id)
        }

        fn restore(&self, selection: &str) -> bool {
            self.run("restore", selection)
        }

        fn init(&self) -> bool {
            self.run("init", "")
        }

        fn start(&self) -> bool {
            self.run("start", "")
        }
    }

    const RESUMED_ON_PREVIOUS_DEVICE: [&str; 4] = ["stop", "restore index:0", "init", "start"];

    #[test]
    fn selects_without_restarting_when_audio_is_not_active() {
        let steps = FakeSteps::new(false, &[]);

        assert_eq!(switch_device(&steps, "new-device"), Ok(()));
        assert_eq!(steps.calls(), ["select new-device"]);
    }

    #[test]
    fn reports_a_failed_selection_when_audio_is_not_active() {
        let steps = FakeSteps::new(false, &[("select", 1)]);

        assert_eq!(switch_device(&steps, "new-device"), Err(AudioError::DeviceNotFound));
        assert_eq!(steps.calls(), ["select new-device"]);
    }

    #[test]
    fn restarts_active_audio_on_the_new_device() {
        let steps = FakeSteps::new(true, &[]);

        assert_eq!(switch_device(&steps, "new-device"), Ok(()));
        assert_eq!(steps.calls(), ["stop", "select new-device", "init", "start"]);
    }

    #[test]
    fn keeps_audio_running_when_it_cannot_be_stopped() {
        let steps = FakeSteps::new(true, &[("stop", 1)]);

        assert_eq!(
            switch_device(&steps, "new-device"),
            Err(AudioError::OperationFailed("stop_recording failed".to_string()))
        );
        assert_eq!(steps.calls(), ["stop"]);
    }

    #[test]
    fn resumes_on_the_previous_device_when_selection_fails() {
        let steps = FakeSteps::new(true, &[("select", 1)]);

        assert_eq!(switch_device(&steps, "new-device"), Err(AudioError::DeviceNotFound));
        assert_eq!(steps.calls()[..2], ["stop", "select new-device"]);
        assert_eq!(steps.calls()[2..], RESUMED_ON_PREVIOUS_DEVICE);
    }

    #[test]
    fn resumes_on_the_previous_device_when_initialization_fails() {
        let steps = FakeSteps::new(true, &[("init", 1)]);

        assert_eq!(
            switch_device(&steps, "new-device"),
            Err(AudioError::OperationFailed("init_recording failed".to_string()))
        );
        assert_eq!(steps.calls()[..3], ["stop", "select new-device", "init"]);
        assert_eq!(steps.calls()[3..], RESUMED_ON_PREVIOUS_DEVICE);
    }

    #[test]
    fn resumes_on_the_previous_device_when_starting_fails() {
        let steps = FakeSteps::new(true, &[("start", 1)]);

        assert_eq!(
            switch_device(&steps, "new-device"),
            Err(AudioError::OperationFailed("start_recording failed".to_string()))
        );
        assert_eq!(steps.calls()[..4], ["stop", "select new-device", "init", "start"]);
        assert_eq!(steps.calls()[4..], RESUMED_ON_PREVIOUS_DEVICE);
    }

    #[test]
    fn resumes_on_the_platform_default_when_the_previous_device_is_gone() {
        let steps = FakeSteps::new(true, &[("init", 1), ("restore", 1)]);

        assert_eq!(
            switch_device(&steps, "new-device"),
            Err(AudioError::OperationFailed("init_recording failed".to_string()))
        );
        assert_eq!(steps.calls()[..3], ["stop", "select new-device", "init"]);
        assert_eq!(
            steps.calls()[3..],
            ["stop", "restore index:0", "stop", "restore", "init", "start"]
        );
    }

    #[test]
    fn reports_the_switch_failure_when_resuming_also_fails() {
        let steps = FakeSteps::new(true, &[("select", 1), ("init", 1), ("init", 2)]);

        assert_eq!(switch_device(&steps, "new-device"), Err(AudioError::DeviceNotFound));
        assert_eq!(
            steps.calls(),
            [
                "stop",
                "select new-device",
                "stop",
                "restore index:0",
                "init",
                "stop",
                "restore",
                "init"
            ]
        );
    }
}
