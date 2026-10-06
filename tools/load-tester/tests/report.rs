use std::path::Path;

use load_tester::report::{read_file, Capacity, Verdict};

fn fixture() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ramp.jsonl"))
}

#[test]
fn ramp_fixture_yields_pass_then_fail_then_invalid() {
    let report = read_file(fixture(), |_| Ok(())).expect("fixture parses");
    assert_eq!(report.steps.len(), 3);

    assert!(matches!(report.steps[0].verdict, Verdict::Pass), "{:?}", report.steps[0].verdict);

    let Verdict::Fail(breaches) = &report.steps[1].verdict else {
        panic!("step 2 should fail, got {:?}", report.steps[1].verdict)
    };
    assert!(breaches.iter().any(|b| b.starts_with("audio MOS p95")), "{breaches:?}");
    assert!(breaches.iter().any(|b| b.starts_with("video stalled p95")), "{breaches:?}");
    assert_eq!((report.steps[1].server_poor, report.steps[1].server_lost), (2, 1));

    let Verdict::Invalid(issues) = &report.steps[2].verdict else {
        panic!("step 3 should be invalid, got {:?}", report.steps[2].verdict)
    };
    assert!(issues.contains(&"worker_thread 97% on worker 1".to_string()), "{issues:?}");

    assert_eq!(report.capacity, Some(Capacity::SfuLimited { ok: Some(4), failed_at: 8 }));
}
