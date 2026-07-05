# Wake word test fixtures

Place WAV files here for integration testing.

- `positive.wav` — ~2s recording of someone saying "Hey LiveKit". Should score >= 0.5.
- `negative.wav` — ~2s recording of non-wake-word audio (silence, music, unrelated speech). Should score < 0.5.

Requirements:
- Format: 16-bit PCM WAV (mono or stereo; stereo is down-mixed to mono)
- Sample rate: 16000 Hz recommended (other standard rates are resampled internally)
- Duration: ~2 seconds minimum
