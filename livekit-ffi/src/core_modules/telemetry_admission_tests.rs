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

/// The count is independent of lifecycle ownership and survives replacement of the runner.
fn admitted(slot: &LifecycleSlot) -> usize {
    slot.admission.get().map_or(0, |state| state.active.load(Ordering::SeqCst))
}

/// Three reservations block construction; the fourth returns without constructing anything.
#[test]
fn admission_precedes_construction_and_unwind_releases_its_reservation() {
    let slot = Arc::new(LifecycleSlot::new());
    let (entered, arrivals) = std::sync::mpsc::sync_channel(3);
    let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    *slot.before_construct.lock().expect("hook") = Some(Arc::new({
        let gate = gate.clone();
        move || {
            let _ = entered.send(());
            let (lock, ready) = &*gate;
            let mut open = lock.lock().expect("gate");
            while !*open {
                open = ready.wait(open).expect("gate");
            }
        }
    }));
    let mut workers = Vec::new();
    let mut weak = Vec::new();
    for _ in 0..3 {
        let slot = slot.clone();
        let transport = TelemetryExportQueue::new();
        weak.push(Arc::downgrade(&transport));
        workers.push(std::thread::spawn(move || {
            let lease = slot.reserve()?;
            slot.before_construction();
            let pipeline = livekit_telemetry::Telemetry::new(TelemetryConfig::default(), transport);
            drop(pipeline);
            drop(lease);
            Ok::<_, TelemetryError>(())
        }));
    }
    let arrived = (0..3).all(|_| arrivals.recv_timeout(FIRST_UPLOAD).is_ok());
    let before = slot.constructed.load(Ordering::SeqCst);
    let full = admitted(&slot);
    let fourth = slot.reserve();
    let (lock, ready) = &*gate;
    *lock.lock().expect("gate") = true;
    ready.notify_all();
    let joined: Vec<_> = workers.into_iter().map(|worker| worker.join()).collect();
    let after = slot.constructed.load(Ordering::SeqCst);
    drop(fourth.as_ref().ok().cloned());
    let refused = matches!(fourth, Err(TelemetryError::AtCapacity));
    drop(fourth);
    let empty = admitted(&slot);
    *slot.before_construct.lock().expect("hook") = None;
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _lease = slot.reserve().expect("slot");
        panic!("partial construction probe");
    }));
    assert!(arrived && joined.iter().all(|result| matches!(result, Ok(Ok(())))));
    assert_eq!((before, full, after, empty), (0, 3, 3, 0));
    assert!(refused && unwound.is_err());
    assert_eq!(admitted(&slot), 0);
    assert!(weak.iter().all(|weak| weak.upgrade().is_none()));
}

/// A task that blocks in its destructor must retain admission until destruction returns.
#[test]
fn admission_survives_actual_aborted_future_destruction_and_slot_restart() {
    struct BlockDrop {
        entered: std::sync::mpsc::SyncSender<()>,
        release: std::sync::mpsc::Receiver<()>,
        marker: Arc<()>,
    }
    impl Drop for BlockDrop {
        fn drop(&mut self) {
            let _ = self.entered.send(());
            let _ = self.release.recv_timeout(FIRST_UPLOAD);
            let _ = &self.marker;
        }
    }
    let tasks = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");
    let slot = slot_on(&tasks);
    let (entered, dropped) = std::sync::mpsc::sync_channel(1);
    let (release, wait) = std::sync::mpsc::sync_channel(1);
    let marker = Arc::new(());
    let weak = Arc::downgrade(&marker);
    let guard = BlockDrop { entered, release: wait, marker };
    let lease = slot.reserve().expect("slot");
    let second = slot.reserve().expect("second");
    let third = slot.reserve().expect("third");
    let task = tasks.spawn(AdmittedTask {
        future: Box::pin(async move {
            let _guard = guard;
            std::future::pending::<()>().await;
        }),
        _lease: lease,
    });
    task.abort();
    let dropping = dropped.recv_timeout(FIRST_UPLOAD).is_ok();
    // Start and end an unrelated lifecycle while destruction remains blocked. A restart cannot
    // reset the slot's independent admission count.
    runtime().block_on(async {
        slot.send(Command::Barrier(oneshot::channel().0));
        shutdown_in(&slot, || ()).await;
    });
    let still_full = matches!(slot.reserve(), Err(TelemetryError::AtCapacity));
    let retained = weak.upgrade().is_some();
    let _ = release.send(());
    let joined = runtime().block_on(task);
    let after = admitted(&slot);
    drop((second, third));
    let tasks_left = tasks.metrics().num_alive_tasks();
    assert!(dropping && still_full && retained);
    assert!(joined.is_err() && weak.upgrade().is_none());
    assert_eq!((after, admitted(&slot), tasks_left), (2, 0, 0));
}

/// Failed command delivery and lost acknowledgements are errors, not consent refusal.
#[test]
fn admission_failed_delivery_and_ack_release_owned_resources() {
    for ack_loss in [false, true] {
        let tasks = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("runtime");
        let slot = slot_on(&tasks);
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let consume = if ack_loss {
            tasks.spawn(async move {
                drop(receiver.recv().await);
            })
        } else {
            drop(receiver);
            tasks.spawn(async {})
        };
        *slot.lock() = Some(LifecycleHandle {
            commands: sender,
            task: Owned(consume),
            generation: 1,
            sent: 0,
        });
        let transport = TelemetryExportQueue::new();
        let weak = Arc::downgrade(&transport);
        let (telemetry, exporter) =
            livekit_telemetry::Telemetry::new(TelemetryConfig::default(), transport);
        let applied = Arc::new(AtomicBool::new(false));
        let apply = applied.clone();
        let result = configure_in(
            &slot,
            telemetry,
            exporter,
            slot.reserve().expect("slot"),
            Installation {
                apply: Box::new(move |_| {
                    apply.store(true, Ordering::SeqCst);
                    true
                }),
                rollback: Box::new(|| ()),
            },
        );
        if let Some(handle) = slot.lock().take() {
            runtime().block_on(handle.end());
        }
        assert!(matches!(result, Err(TelemetryError::LifecycleUnavailable)), "ack loss={ack_loss}");
        assert!(!applied.load(Ordering::SeqCst));
        assert_eq!(admitted(&slot), 0);
        assert!(weak.upgrade().is_none());
        assert_eq!(tasks.metrics().num_alive_tasks(), 0);
    }
}

/// Failed Run delivery rolls back the installed owner before releasing admission or replying.
#[test]
fn admission_failed_run_rolls_back_and_dropped_ack_uses_normal_stop() {
    for ack_loss in [false, true] {
        let tasks = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("runtime");
        let slot = slot_on(&tasks);
        let (commands, receiver) = mpsc::unbounded_channel();
        let (exporters, mut steps) = mpsc::unbounded_channel();
        let installed = Arc::new(Installed::default());
        let rollback = installed.clone();
        let apply = installed.clone();
        let queue = TelemetryExportQueue::new();
        let weak = Arc::downgrade(&queue);
        let (telemetry, exporter) =
            livekit_telemetry::Telemetry::new(TelemetryConfig::default(), queue);
        let (done, ack) = std::sync::mpsc::sync_channel(1);
        if !ack_loss {
            steps.close();
        }
        let runner = tasks.spawn(async {});
        let lifecycle = tasks.spawn(Lifecycle { commands: receiver, exporters }.run(Owned(runner)));
        let ack = if ack_loss {
            drop(ack);
            None
        } else {
            Some(ack)
        };
        commands
            .send(Command::Configure {
                installation: Installation {
                    apply: Box::new(move |telemetry| {
                        apply.lock().expect("install").replace(telemetry);
                        true
                    }),
                    rollback: Box::new(move || {
                        rollback.lock().expect("rollback").take();
                    }),
                },
                telemetry,
                exporter: Box::new(exporter),
                done,
                lease: slot.reserve().expect("slot"),
            })
            .unwrap_or_else(|_| panic!("command"));
        let reply = ack.and_then(|ack| ack.recv_timeout(FIRST_UPLOAD).ok());
        let observed = if ack_loss {
            runtime().block_on(async {
                let first = tokio::time::timeout(FIRST_UPLOAD, steps.recv()).await;
                // Destroy the queued Run before the stop, just as a failed runner would.
                let run = matches!(first, Ok(Some(Step::Run(..))));
                drop(first);
                let next = tokio::time::timeout(FIRST_UPLOAD, steps.recv()).await;
                let stop = matches!(next, Ok(Some(Step::Stop(_))));
                drop(next);
                run && stop
            })
        } else {
            matches!(reply, Some(Err(TelemetryError::LifecycleUnavailable)))
        };
        drop(commands);
        let joined = runtime().block_on(lifecycle);
        drop(steps);
        let metrics = tasks.metrics();
        tasks.shutdown_timeout(END_BOUND);
        assert!(observed && joined.is_ok(), "ack loss={ack_loss}");
        assert!(installed.lock().expect("install").is_none());
        assert_eq!(admitted(&slot), 0);
        assert!(weak.upgrade().is_none());
        assert_eq!(metrics.num_alive_tasks(), 0);
    }
}

/// Real exported calls keep old pulled requests answerable at capacity in memory and file modes.
/// Called under PROCESS_PIPELINE before the existing irreversible opt-out test.
fn admission_public_pulled_controls() {
    for disk in [false, true] {
        let parent =
            std::env::temp_dir().join(format!("lk-admission-{}-{disk}", std::process::id()));
        let _ = std::fs::remove_dir_all(&parent);
        std::fs::create_dir_all(&parent).expect("parent");
        let config = || TelemetryConfig {
            resource: Vec::new(),
            export_timeout_ms: 30_000,
            storage_dir: disk.then(|| parent.join("accepted").to_string_lossy().into_owned()),
            ..Default::default()
        };
        let before = LIFECYCLE.constructed.load(Ordering::SeqCst);
        let queues_before = LIFECYCLE.queues_constructed.load(Ordering::SeqCst);
        let first = telemetry_configure_pulled(config(), Vec::new()).expect("first");
        let room = telemetry_scope().expect("room");
        room.set_server("wss://p.livekit.cloud".into(), granted_token());
        room.emit_custom("held".into(), HashMap::new());
        let flushing = crate::runtime::runtime().spawn(telemetry_flush());
        let request =
            runtime().block_on(async { tokio::time::timeout(FIRST_UPLOAD, first.next()).await });
        let second = telemetry_configure_pulled(config(), Vec::new());
        let second_room = telemetry_scope();
        if let Some(room) = &second_room {
            room.set_server("wss://p.livekit.cloud".into(), granted_token());
            room.emit_custom("second".into(), HashMap::new());
        }
        let third = telemetry_configure_pulled(config(), Vec::new());
        let third_room = telemetry_scope();
        if let Some(room) = &third_room {
            room.set_server("wss://p.livekit.cloud".into(), granted_token());
            room.emit_custom("third".into(), HashMap::new());
        }
        let rejected_dir = parent.join("rejected");
        let fourth = telemetry_configure_pulled(
            TelemetryConfig {
                storage_dir: Some(rejected_dir.to_string_lossy().into_owned()),
                ..config()
            },
            Vec::new(),
        );
        let normal = telemetry_configure(config(), Some(TelemetryExportQueue::new()), Vec::new());
        let count = admitted(&LIFECYCLE);
        let constructors = LIFECYCLE.constructed.load(Ordering::SeqCst) - before;
        let queues = LIFECYCLE.queues_constructed.load(Ordering::SeqCst) - queues_before;
        let intact = !first.finished.load(Ordering::SeqCst) && !rejected_dir.exists();
        // Controls bypass capacity and preserve FIFO without shortening a held request's drain.
        runtime().block_on(async {
            for _ in 0..64 {
                barrier(&LIFECYCLE).await;
            }
        });
        let mut stopped = Vec::new();
        for _ in 0..64 {
            let (done, ack) = oneshot::channel();
            LIFECYCLE.send(Command::Shutdown { uninstall: Box::new(|| ()), done });
            stopped.push(ack);
        }
        let still_held = !first.inflight.lock().expect("inflight").is_empty();
        if let Ok(Some(request)) = &request {
            first.complete(request.id, ExportResponse::accepted());
        }
        let mut queues_owned = vec![first];
        if let Ok(queue) = second {
            queues_owned.push(queue);
        }
        if let Ok(queue) = third {
            queues_owned.push(queue);
        }
        let weak: Vec<_> = queues_owned.iter().map(Arc::downgrade).collect();
        runtime().block_on(async {
            // Keep servicing all admitted hosts through their normal drain, including health
            // records created after the first acknowledgement. No timeout drives progress.
            let mut servers = Vec::new();
            for queue in &queues_owned {
                let queue = queue.clone();
                servers.push(tokio::spawn(async move {
                    while let Some(request) = queue.next().await {
                        queue.complete(request.id, ExportResponse::accepted());
                    }
                }));
            }
            telemetry_shutdown().await;
            for ack in stopped {
                ack.await.expect("control acknowledged");
            }
            let _ = flushing.await;
            for queue in &queues_owned {
                queue.finish();
            }
            for server in servers {
                server.await.expect("host joined");
            }
        });
        // Dropping a live scope emits an end record. Keep it until normal shutdown has joined
        // rather than accidentally creating another request that this host is not serving.
        drop((room, second_room, third_room));
        let each_ran = queues_owned.iter().all(|queue| queue.queued.load(Ordering::SeqCst) > 0);
        for queue in &queues_owned {
            queue.finish();
        }
        drop(queues_owned);
        let no_tasks = LIFECYCLE.lock().is_none();
        let clean = admitted(&LIFECYCLE) == 0 && weak.iter().all(|queue| queue.upgrade().is_none());
        let _ = std::fs::remove_dir_all(&parent);
        assert!(matches!(request, Ok(Some(_))), "disk={disk}: request reached host");
        assert!(matches!(fourth, Err(TelemetryError::AtCapacity)));
        assert!(matches!(normal, Err(TelemetryError::AtCapacity)));
        assert_eq!((count, constructors, queues), (3, 3, 3));
        assert!(intact && still_held && clean && no_tasks && each_ran, "disk={disk}");
    }
}

/// Run in a fresh process for each irreversible opt-out boundary, by the recorded gate driver.
#[test]
#[ignore = "one-way process consent; run with the phase4 driver for each boundary"]
fn admission_child_consent_boundaries() {
    let case = std::env::var("LK_ADMISSION_BOUNDARY").expect("boundary supplied by gate driver");
    if case == "zero" {
        runtime().block_on(telemetry_shutdown());
        assert_eq!(LIFECYCLE.generations.load(Ordering::SeqCst), 0);
        assert!(LIFECYCLE.admission.get().is_none());
        assert!(LIFECYCLE.lock().is_none());
        return;
    }
    let dir = std::env::temp_dir().join(format!("lk-admission-consent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("old-secret"), b"pending").expect("old data");
    let before = LIFECYCLE.constructed.load(Ordering::SeqCst);
    let mut leases = Vec::new();
    let mut worker = None;
    let mut release = None;
    let mut entered = None;
    if case == "reserved" {
        let (arrived, receive) = std::sync::mpsc::sync_channel(1);
        let (open, waiting) = std::sync::mpsc::sync_channel(1);
        let waiting = Mutex::new(waiting);
        *LIFECYCLE.before_construct.lock().expect("hook") = Some(Arc::new(move || {
            let _ = arrived.send(());
            let _ = waiting.lock().expect("release").recv_timeout(FIRST_UPLOAD);
        }));
        let path = dir.to_string_lossy().into_owned();
        worker = Some(std::thread::spawn(move || {
            telemetry_configure_pulled(
                TelemetryConfig {
                    storage_dir: Some(path),
                    resource: Vec::new(),
                    ..Default::default()
                },
                Vec::new(),
            )
        }));
        entered = Some(receive.recv_timeout(FIRST_UPLOAD).is_ok());
        release = Some(open);
    } else {
        for _ in 0..3 {
            leases.push(LIFECYCLE.reserve().expect("slot"));
        }
    }
    // Opt-out does not wait for the reserved caller, and controls bypass a full counter.
    telemetry_disable();
    if let Some(release) = release {
        let _ = release.send(());
    }
    let result = if let Some(worker) = worker {
        worker.join().expect("constructor joined")
    } else {
        telemetry_configure_pulled(
            TelemetryConfig {
                storage_dir: Some(dir.to_string_lossy().into_owned()),
                ..Default::default()
            },
            Vec::new(),
        )
    };
    *LIFECYCLE.before_construct.lock().expect("hook") = None;
    let queue = result.expect("consent is a successful refusal");
    let weak = Arc::downgrade(&queue);
    let finished = queue.finished.load(Ordering::SeqCst) && queue.try_next().is_none();
    let count_before_release = admitted(&LIFECYCLE);
    drop(leases);
    runtime().block_on(telemetry_shutdown());
    let empty = std::fs::read_dir(&dir).expect("dir").count() == 0;
    let constructors = LIFECYCLE.constructed.load(Ordering::SeqCst) - before;
    drop(queue);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(finished && empty && weak.upgrade().is_none());
    assert_eq!(admitted(&LIFECYCLE), 0);
    assert!(LIFECYCLE.lock().is_none());
    assert!(telemetry_scope().is_none());
    if case == "reserved" {
        assert_eq!(entered, Some(true));
        assert_eq!((constructors, count_before_release), (1, 0));
    } else {
        assert_eq!((constructors, count_before_release), (0, 3));
    }
}
