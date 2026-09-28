use super::*;

#[test]
fn bounded_profile_keeps_failed_and_cancelled_stage_outcomes() {
    let stage = Stage::AdmissionGlobalQueue;
    Timer::start(stage).complete(false);
    drop(Timer::start(Stage::ServiceSlot));

    let report = encode(take());

    assert_eq!(report["schema"], 2);
    assert_eq!(
        report["stages"].as_object().unwrap().len(),
        super::super::STAGE_COUNT
    );
    assert_eq!(report["stages"]["admission_global_queue"]["error"], 1);
    assert_eq!(report["stages"]["admission_global_queue"]["cancelled"], 0);
    assert_eq!(report["stages"]["service_slot"]["error"], 0);
    assert_eq!(report["stages"]["service_slot"]["cancelled"], 1);
}
