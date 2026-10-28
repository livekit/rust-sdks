"""Compare two RTC libraries using simultaneous synthetic four-room workloads.

Requires livekit, livekit-api and psutil, plus the normal LIVEKIT_* credentials.
Each trial uses fresh rooms. Only the agent process CPU is measured.
"""

import argparse
import asyncio
import json
import math
import os
import random
import subprocess
import sys
import threading
import time
import uuid
from array import array
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

_ABORT = threading.Event()


def signal_frame(rtc):
    pcm = array(
        "h",
        (
            int(
                7000 * math.sin(i * 2 * math.pi * 120 / 24000)
                + 3000 * math.sin(i * 2 * math.pi * 500 / 24000)
            )
            for i in range(1200)
        ),
    )
    return rtc.AudioFrame(pcm.tobytes(), 24000, 1, 1200)


async def participant(args):
    from livekit import api, rtc

    rooms, sources, tracks, tasks, published = [], [], [], [], []
    received, subscribed = {}, set()
    frame = signal_frame(rtc)

    async def feed(source):
        while True:
            await source.capture_frame(frame)

    async def listen(track, index):
        stream = rtc.AudioStream(
            track, sample_rate=16000, num_channels=1, frame_size_ms=50
        )
        try:
            async for event in stream:
                received[index] = (
                    received.get(index, 0) + event.frame.samples_per_channel
                )
        finally:
            await stream.aclose()

    async def connect(index):
        room = rtc.Room()
        rooms.append(room)

        @room.on("track_subscribed")
        def on_track(track, publication, who):
            tracks.append(track)
            subscribed.add(index)
            tasks.append(asyncio.create_task(listen(track, index)))

        token = (
            api.AccessToken()
            .with_identity(args.role)
            .with_grants(api.VideoGrants(room_join=True, room=f"{args.room}-{index}"))
            .to_jwt()
        )
        await room.connect(
            os.environ["LIVEKIT_URL"],
            token,
            options=rtc.RoomOptions(single_peer_connection=bool(args.single_pc)),
        )
        source = rtc.AudioSource(24000, 1, queue_size_ms=1000)
        sources.append(source)
        track = rtc.LocalAudioTrack.create_audio_track("synthetic", source)
        tracks.append(track)
        published.append(track)
        await room.local_participant.publish_track(
            track,
            rtc.TrackPublishOptions(
                source=rtc.TrackSource.SOURCE_MICROPHONE,
                dtx=bool(args.dtx),
                red=bool(args.red),
                audio_encoding={"max_bitrate": 48000},
            ),
        )
        # Incoming audio remains active while the agent's outgoing source is idle.
        if args.role == "peer" or args.mode == "speech":
            tasks.append(asyncio.create_task(feed(source)))

    async def codecs():
        from google.protobuf.json_format import MessageToDict

        result = []
        for track in published:
            values = []
            for row in await track.get_stats():
                data = MessageToDict(row, preserving_proto_field_name=True)
                if "codec" in data:
                    codec = data["codec"]["codec"]
                    values.append(
                        {
                            key: codec.get(key)
                            for key in (
                                "payload_type",
                                "mime_type",
                                "clock_rate",
                                "channels",
                                "sdp_fmtp_line",
                            )
                        }
                    )
            result.append(
                sorted(values, key=lambda value: json.dumps(value, sort_keys=True))
            )
        return result

    initial_codecs = []
    try:
        await asyncio.gather(*(connect(i) for i in range(args.sessions)))
        while len(subscribed) != args.sessions:
            await asyncio.sleep(0.05)
        await asyncio.sleep(1)
        initial_codecs = await codecs()
        (args.out / f"{args.label}.ready").write_text(json.dumps(initial_codecs))
        while not (args.out / "stop").exists():
            await asyncio.sleep(0.05)
    finally:
        try:
            final_codecs = await codecs()
        except Exception:
            final_codecs = []
        for task in tasks:
            task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)
        await asyncio.gather(
            *(room.disconnect() for room in rooms), return_exceptions=True
        )
        await asyncio.gather(
            *(source.aclose() for source in sources), return_exceptions=True
        )
        (args.out / f"{args.label}.json").write_text(
            json.dumps(
                {
                    "received_samples": received,
                    "initial_codecs": initial_codecs,
                    "final_codecs": final_codecs,
                }
            )
        )


def trial(args, mode, repeat):
    import psutil

    if _ABORT.is_set():
        return
    out = args.out / f"{mode}-r{repeat}"
    out.mkdir(parents=True)
    processes, logs = {}, []
    variants = [("baseline", args.baseline), ("candidate", args.candidate)]
    random.Random(repeat).shuffle(variants)
    run_id = uuid.uuid4().hex[:16]
    try:
        for variant, library in variants:
            for role in ("peer", "agent"):
                label = f"{variant}-{role}"
                log = (out / f"{label}.log").open("w")
                logs.append(log)
                env = dict(os.environ)
                # Use the same installed receiver/peer library in both variants.
                if role == "agent":
                    env["LIVEKIT_LIB_PATH"] = str(library.resolve())
                else:
                    env.pop("LIVEKIT_LIB_PATH", None)
                command = [
                    sys.executable,
                    str(Path(__file__).resolve()),
                    "--role",
                    role,
                    "--out",
                    str(out.resolve()),
                    "--room",
                    f"sched-{run_id}-{variant}",
                    "--label",
                    label,
                    "--mode",
                    mode,
                    "--sessions",
                    str(args.sessions),
                    "--dtx",
                    str(args.dtx),
                    "--red",
                    str(args.red),
                    "--single-pc",
                    str(args.single_pc),
                ]
                processes[label] = subprocess.Popen(
                    command, env=env, stdout=log, stderr=subprocess.STDOUT
                )
        deadline = time.monotonic() + 40
        while not all((out / f"{label}.ready").exists() for label in processes):
            if _ABORT.is_set():
                raise RuntimeError("Another RTC trial failed")
            if any(process.poll() is not None for process in processes.values()):
                raise RuntimeError(f"RTC setup failed; inspect {out}")
            if time.monotonic() > deadline:
                raise TimeoutError(f"RTC setup timed out; inspect {out}")
            time.sleep(0.05)
        initial = {
            name: json.loads((out / f"{name}.ready").read_text()) for name in processes
        }
        if initial["baseline-agent"] != initial["candidate-agent"]:
            raise RuntimeError(f"Negotiated encoder formats differ; exclude {out}")
        time.sleep(3)

        def snapshot():
            return {
                name: sum(psutil.Process(process.pid).cpu_times()[:2])
                for name, process in processes.items()
                if name.endswith("-agent")
            }

        before = snapshot()
        start = time.monotonic()
        time.sleep(args.duration)
        after = snapshot()
        elapsed = time.monotonic() - start
        (out / "stop").touch()
        for process in processes.values():
            process.wait(timeout=15)
        codes = {name: process.returncode for name, process in processes.items()}
        if any(codes.values()):
            raise RuntimeError(f"RTC participant failed: {codes}")
        work = {
            name: json.loads((out / f"{name}.json").read_text()) for name in processes
        }
        for name in work:
            if work[name]["initial_codecs"] != work[name]["final_codecs"]:
                raise RuntimeError(f"Codec negotiation changed during trial: {name}")
        result = {
            "mode": mode,
            "repeat": repeat,
            "sessions": args.sessions,
            "dtx": args.dtx,
            "red": args.red,
            "single_pc": args.single_pc,
            "wall_s": elapsed,
            "cpu_mc": {
                name: (after[name] - before[name]) / elapsed * 1000 for name in before
            },
            "exit_codes": codes,
            "work": work,
        }
        (out / "result.json").write_text(json.dumps(result, indent=2))
        print(mode, repeat, result["cpu_mc"], flush=True)
    except BaseException:
        _ABORT.set()
        raise
    finally:
        (out / "stop").touch()
        for process in processes.values():
            if process.poll() is None:
                process.terminate()
        for process in processes.values():
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        for log in logs:
            log.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--sessions", type=int, default=4)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--duration", type=float, default=15)
    parser.add_argument("--parallel", type=int, default=2)
    parser.add_argument("--dtx", type=int, choices=[0, 1], default=1)
    parser.add_argument("--red", type=int, choices=[0, 1], default=1)
    parser.add_argument("--single-pc", type=int, choices=[0, 1], default=0)
    parser.add_argument("--role", choices=["matrix", "peer", "agent"], default="matrix")
    parser.add_argument("--room")
    parser.add_argument("--label")
    parser.add_argument("--mode", choices=["speech", "idle"], default="speech")
    args = parser.parse_args()
    if args.role != "matrix":
        asyncio.run(participant(args))
        return
    if not args.baseline or not args.candidate:
        parser.error("matrix requires --baseline and --candidate")
    if min(args.sessions, args.repeats, args.parallel, args.duration) <= 0:
        parser.error("sessions, repeats, parallel and duration must be positive")
    for library in (args.baseline, args.candidate):
        if not library.is_file():
            parser.error(f"missing library: {library}")
    cases = [
        (mode, repeat) for repeat in range(args.repeats) for mode in ("speech", "idle")
    ]
    random.Random(123).shuffle(cases)
    with ThreadPoolExecutor(max_workers=args.parallel) as pool:
        futures = [pool.submit(trial, args, *case) for case in cases]
        for future in as_completed(futures):
            future.result()


if __name__ == "__main__":
    main()
