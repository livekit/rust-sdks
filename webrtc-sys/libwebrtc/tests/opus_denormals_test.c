/* Copyright 2026 LiveKit, Inc.
 * Licensed under the Apache License, Version 2.0.
 *
 * Standalone test: compile with Opus headers and the rebuilt static libopus
 * objects. Exercises public API success/error paths and repeated teardown.
 */
#include <stdio.h>
#include <opus.h>
#if defined(__x86_64__)
#include <xmmintrin.h>

#define CHECK(expr) do { if (!(expr)) { \
    fprintf(stderr, "failed line %d: %s\n", __LINE__, #expr); return 1; \
} } while (0)

/* Exception status bits may change; floating-point control bits must not. */
static int same_controls(unsigned expected) {
    return (_mm_getcsr() & ~0x3fu) == (expected & ~0x3fu);
}

/* Every cycle owns and destroys one encoder/decoder, including error probes. */
static int exercise(unsigned mode) {
    int cycle, i, error;
    opus_int16 input[960], output[960];
    unsigned char packet[4000], malformed[1] = {255};
    for (i = 0; i < 960; ++i) input[i] = (opus_int16)((i * 197) % 20000 - 10000);
    _mm_setcsr(mode);
    for (cycle = 0; cycle < 64; ++cycle) {
        OpusEncoder *encoder = opus_encoder_create(48000, 1, OPUS_APPLICATION_VOIP, &error);
        OpusDecoder *decoder;
        int len;
        CHECK(encoder && error == OPUS_OK);
        decoder = opus_decoder_create(48000, 1, &error);
        CHECK(decoder && error == OPUS_OK);
        CHECK(opus_encoder_ctl(encoder, OPUS_SET_INBAND_FEC(1)) == OPUS_OK);
        CHECK(opus_encoder_ctl(encoder, OPUS_SET_DTX(1)) == OPUS_OK);
        len = opus_encode(encoder, input, 960, packet, sizeof(packet));
        CHECK(len > 0 && same_controls(mode));
        CHECK(opus_decode(decoder, packet, len, output, 960, 0) == 960);
        CHECK(same_controls(mode));
        CHECK(opus_decode(decoder, NULL, 0, output, 960, 0) == 960);
        CHECK(same_controls(mode));
        CHECK(opus_decode(decoder, packet, len, output, 960, 1) == 960);
        CHECK(same_controls(mode));
        CHECK(opus_encode(encoder, input, 960, packet, 0) < 0);
        CHECK(same_controls(mode));
        CHECK(opus_decode(decoder, malformed, sizeof(malformed), output, 960, 0) < 0);
        CHECK(same_controls(mode));
        opus_decoder_destroy(decoder);
        opus_encoder_destroy(encoder);
        CHECK(same_controls(mode));
        encoder = opus_encoder_create(12345, 1, OPUS_APPLICATION_VOIP, &error);
        CHECK(!encoder && error < 0 && same_controls(mode));
    }
    return 0;
}
#endif

int main(void) {
#if defined(__x86_64__)
    unsigned saved = _mm_getcsr();
    int result = exercise(saved & ~0x8040u);
    if (!result) result = exercise(saved | 0x8040u);
    _mm_setcsr(saved);
    if (result) return result;
    puts("128 Opus lifecycle cycles passed; FP controls restored on success and errors");
#else
    puts("SKIP: x86-64 floating-point control test");
#endif
    return 0;
}
