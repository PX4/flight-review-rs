use super::{analyze_fixture_for, analyzer, feed, MessageBuilder as B};
use crate::diagnostics::{Analyzer, AnomalyKind, Diagnostic, Evidence, Severity};

fn arm(a: &mut dyn Analyzer) {
    feed(
        a,
        B::new("vehicle_status")
            .timestamp(0)
            .field_u8("arming_state", 2),
        0,
    );
}

fn battery(ts: u64, voltage: f32, cells: u8, connected: bool) -> B {
    B::new("battery_status")
        .timestamp(ts)
        .field_f32("voltage_v", voltage)
        .field_u8("cell_count", cells)
        .field_bool("connected", connected)
}

fn gps(ts: u64, eph: f32, satellites: u8, fix: u8) -> B {
    B::new("vehicle_gps_position")
        .timestamp(ts)
        .field_f32("eph", eph)
        .field_f32("epv", 2.0)
        .field_u8("satellites_used", satellites)
        .field_u8("fix_type", fix)
}

fn output(ts: u64, value: f32) -> B {
    B::new("actuator_outputs")
        .timestamp(ts)
        .field_u32("noutputs", 1)
        .field_f32("output[0]", value)
}

fn rc(ts: u64, lost: bool, failsafe: bool) -> B {
    B::new("input_rc")
        .timestamp(ts)
        .field_bool("rc_lost", lost)
        .field_bool("rc_failsafe", failsafe)
}

fn ekf(ts: u64, ratio: f32) -> B {
    B::new("estimator_status")
        .timestamp(ts)
        .field_f32("vel_test_ratio", ratio)
        .field_f32("pos_test_ratio", 0.2)
        .field_f32("hgt_test_ratio", 0.3)
}

fn selector(ts: u64, count: u32, ratio: f32) -> B {
    B::new("estimator_selector_status")
        .timestamp(ts)
        .field_u8("instances_available", 2)
        .field_u8("primary_instance", (count % 2) as u8)
        .field_u32("instance_changed_count", count)
        .field_f32("combined_test_ratio[0]", ratio)
        .field_f32("combined_test_ratio[1]", ratio)
}

fn tecs(ts: u64, value: f32) -> B {
    B::new("tecs_status")
        .timestamp(ts)
        .field_f32("pitch_integ", value)
        .field_f32("pitch_sp_rad", value)
        .field_f32("throttle_integ", 0.1)
}

pub(super) fn positive_case(id: &str) -> Vec<Diagnostic> {
    let mut a = analyzer(id);
    arm(a.as_mut());
    match id {
        "battery_brownout" => feed(a.as_mut(), battery(1_000_000, 12.0, 4, true), 0),
        "motor_failure" => {
            feed(a.as_mut(), output(1_000_000, 1500.0), 0);
            feed(a.as_mut(), output(2_000_000, 0.0), 0);
        }
        "gps_interference" => {
            for i in 1..=10 {
                feed(a.as_mut(), gps(i * 100_000, 1.0, 12, 3), 0);
            }
            feed(a.as_mut(), gps(2_000_000, 20.0, 12, 3), 0);
        }
        "rc_loss" => {
            feed(a.as_mut(), rc(1_000_000, true, false), 0);
            feed(a.as_mut(), rc(7_000_000, false, false), 0);
        }
        "ekf_failure" => {
            for i in 0..=60 {
                feed(a.as_mut(), ekf(i * 100_000, 2.0), 0);
            }
        }
        "ekf_selector_whipsaw" => {
            feed(a.as_mut(), selector(0, 0, 0.2), 0);
            feed(a.as_mut(), selector(8_000_000, 8, 0.2), 0);
        }
        "tecs_nonfinite_pitch" => {
            feed(a.as_mut(), tecs(1_000_000, f32::NAN), 0);
            feed(a.as_mut(), tecs(2_000_000, 0.1), 0);
        }
        _ => panic!("missing positive control for {id}"),
    }
    a.finish()
}

#[test]
fn no_false_positives_with_armed_topic_present_controls() {
    for mut a in crate::diagnostics::create_analyzers() {
        arm(a.as_mut());
        for instance in [0, 1] {
            for i in 0..=60 {
                let ts = i * 100_000;
                let message = match a.id() {
                    "battery_brownout" => battery(ts, 24.0, 6, true),
                    "motor_failure" => output(ts, 4095.0),
                    "gps_interference" => gps(ts, 1.0, 12, 3),
                    "rc_loss" => rc(ts, false, false),
                    "ekf_failure" => ekf(ts, 0.2),
                    "ekf_selector_whipsaw" => selector(ts, 1, 0.2),
                    "tecs_nonfinite_pitch" => tecs(ts, 0.1),
                    id => panic!("missing healthy control for {id}"),
                };
                feed(a.as_mut(), message, instance);
            }
        }
        let id = a.id().to_string();
        assert!(a.finish().is_empty(), "{id} reported healthy measurements");
        assert!(
            !positive_case(&id).is_empty(),
            "{id} did not detect its positive control"
        );
    }
}

#[test]
fn motor_instances_valid_output_count_and_disarm_are_isolated() {
    let mut a = analyzer("motor_failure");
    arm(a.as_mut());
    feed(a.as_mut(), output(1_000_000, 4095.0), 1);
    feed(a.as_mut(), output(1_001_000, 0.0), 0);
    // These are invalid storage slots, not valid outputs.
    feed(
        a.as_mut(),
        B::new("actuator_outputs")
            .timestamp(2_000_000)
            .field_u32("noutputs", 0)
            .field_f32("output[0]", 4095.0),
        2,
    );
    feed(a.as_mut(), output(3_000_000, 0.0), 2);
    feed(a.as_mut(), output(3_000_000, 0.0), 1);
    feed(
        a.as_mut(),
        B::new("vehicle_status")
            .timestamp(4_000_000)
            .field_u8("arming_state", 1),
        0,
    );
    feed(
        a.as_mut(),
        B::new("vehicle_status")
            .timestamp(5_000_000)
            .field_u8("arming_state", 2),
        0,
    );
    feed(a.as_mut(), output(6_000_000, 0.0), 1);
    let diags = a.finish();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].anchor.instance, Some(1));
    assert_eq!(diags[0].timestamp_us, 3_000_000);
    assert_eq!(diags[0].severity, Severity::Warning);
    assert!(!diags[0].summary.contains("failure"));
}

#[test]
fn battery_validity_cells_instances_and_dedup_boundaries() {
    let mut a = analyzer("battery_brownout");
    arm(a.as_mut());
    feed(a.as_mut(), battery(0, 25.0, 6, true), 0);
    feed(a.as_mut(), battery(0, 16.0, 4, true), 1);
    feed(a.as_mut(), battery(1, 0.06, 4, false), 2);
    feed(a.as_mut(), battery(1, 12.0, 0, true), 3);
    feed(a.as_mut(), battery(1, f32::NAN, 4, true), 4);
    for ts in [1_000_000, 30_999_999, 31_000_000] {
        feed(a.as_mut(), battery(ts, 12.0, 4, true), 1);
    }
    let diags = a.finish();
    assert_eq!(diags.len(), 2);
    assert_eq!(diags[0].timestamp_us, 1_000_000);
    assert_eq!(diags[1].timestamp_us, 31_000_000);
    assert!(diags.iter().all(|d| d.anchor.instance == Some(1)));
}

#[test]
fn gps_invalid_startup_does_not_poison_baseline_or_other_instances() {
    let mut a = analyzer("gps_interference");
    for i in 0..10 {
        feed(a.as_mut(), gps(i * 100_000, 4_294_967.5, 0, 0), 0);
        feed(a.as_mut(), gps(i * 100_000, 1.0, 12, 3), 1);
    }
    for i in 10..20 {
        feed(a.as_mut(), gps(i * 100_000, 1.0, 12, 3), 0);
    }
    feed(
        a.as_mut(),
        B::new("vehicle_gps_position").timestamp(2_000_000),
        0,
    );
    feed(a.as_mut(), gps(2_100_000, 20.0, 12, 3), 0);
    feed(a.as_mut(), gps(2_100_000, 1.0, 12, 3), 1);
    let diags = a.finish();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Severity::Critical);
    assert_eq!(diags[0].timestamp_us, 2_100_000);
}

#[test]
fn rc_bool_failsafe_eof_last_signal_and_recovery_boundaries() {
    for (end, expected) in [
        (1_499_999, None),
        (1_500_000, Some(Severity::Warning)),
        (6_000_000, Some(Severity::Critical)),
    ] {
        let mut a = analyzer("rc_loss");
        arm(a.as_mut());
        feed(
            a.as_mut(),
            rc(500_000, false, false).field_u64("timestamp_last_signal", 400_000),
            1,
        );
        // A receiver can keep transmitting frames while reporting failsafe.
        feed(
            a.as_mut(),
            rc(1_000_000, false, true).field_u64("timestamp_last_signal", 400_000),
            1,
        );
        feed(a.as_mut(), rc(end, false, false), 0); // another receiver cannot recover instance 1
        feed(
            a.as_mut(),
            B::new("vehicle_status")
                .timestamp(end)
                .field_u8("arming_state", 2),
            0,
        );
        let diags = a.finish();
        assert_eq!(diags.len(), usize::from(expected.is_some()));
        if let Some(severity) = expected {
            assert_eq!(diags[0].severity, severity);
            assert_eq!(diags[0].anchor.instance, Some(1));
            assert_eq!(
                diags[0].kind,
                AnomalyKind::Region {
                    end_timestamp_us: end
                }
            );
            assert!(matches!(
                diags[0].evidence,
                Evidence::RcLoss {
                    last_signal_timestamp_us: 400_000,
                    ..
                }
            ));
        }
    }
}

#[test]
fn ekf_instances_do_not_reset_each_other_and_gaps_are_not_sustained() {
    let mut a = analyzer("ekf_failure");
    for i in 0..=60 {
        feed(a.as_mut(), ekf(i * 100_000, 2.0), 1);
        feed(a.as_mut(), ekf(i * 100_000, 0.2), 0);
    }
    feed(a.as_mut(), ekf(0, 2.0), 2);
    feed(a.as_mut(), ekf(6_000_000, 2.0), 2);
    let diags = a.finish();
    assert_eq!(diags.len(), 2);
    assert!(diags.iter().all(|d| d.anchor.instance == Some(1)));
    assert_eq!(diags[1].severity, Severity::Critical);
    let real = analyze_fixture_for("ekf_selector_whipsaw.ulg", "ekf_failure");
    assert!(real
        .iter()
        .any(|d| d.timestamp_us == 228_276_951 && d.severity == Severity::Critical));
}

#[test]
fn selector_counts_bounded_deltas_and_allows_escalation() {
    let diags = positive_case("ekf_selector_whipsaw");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Severity::Critical);
    assert!(matches!(
        diags[0].evidence,
        Evidence::EkfSelectorWhipsaw {
            switch_count: 8,
            avg_switch_interval_ms: None,
            ..
        }
    ));
    for degraded in [false, true] {
        let mut a = analyzer("ekf_selector_whipsaw");
        for i in 0..=8 {
            feed(
                a.as_mut(),
                selector(
                    i * 1_000_000,
                    i as u32,
                    if degraded && i >= 4 { 2.0 } else { 0.2 },
                ),
                0,
            );
        }
        let diags = a.finish();
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].severity, Severity::Warning);
        assert_eq!(diags[1].severity, Severity::Critical);
    }
    let mut a = analyzer("ekf_selector_whipsaw");
    feed(a.as_mut(), selector(0, 0, 0.2), 0);
    feed(a.as_mut(), selector(20_000_000, 8, 0.2), 0);
    assert!(
        a.finish().is_empty(),
        "cannot put twenty seconds of unknown timing into ten seconds"
    );
}

#[test]
fn selector_invalid_index_and_counter_reset_are_safe() {
    let mut a = analyzer("ekf_selector_whipsaw");
    feed(a.as_mut(), selector(0, 100, 0.2), 0);
    feed(a.as_mut(), selector(1_000_000, 0, 0.2), 0);
    feed(
        a.as_mut(),
        B::new("estimator_selector_status")
            .timestamp(2_000_000)
            .field_u8("instances_available", 2)
            .field_u8("primary_instance", 255)
            .field_u32("instance_changed_count", 10),
        0,
    );
    assert!(a.finish().is_empty());
}

#[test]
fn tecs_recovery_missing_fields_and_setpoint_anchor() {
    let mut a = analyzer("tecs_nonfinite_pitch");
    feed(a.as_mut(), tecs(1_000_000, 0.1), 0);
    feed(a.as_mut(), tecs(2_000_000, f32::NAN), 0);
    feed(a.as_mut(), tecs(3_000_000, 0.1), 0);
    feed(a.as_mut(), tecs(10_000_000, 0.1), 0);
    feed(
        a.as_mut(),
        B::new("tecs_status")
            .timestamp(4_000_000)
            .field_f32("pitch_sp_rad", f32::INFINITY),
        1,
    );
    feed(a.as_mut(), B::new("tecs_status").timestamp(8_000_000), 1);
    let diags = a.finish();
    assert_eq!(diags.len(), 2);
    assert_eq!(
        diags[0].kind,
        AnomalyKind::Region {
            end_timestamp_us: 3_000_000
        }
    );
    assert!(!diags[0].summary.contains("never recovered"));
    assert_eq!(diags[1].anchor.field, "pitch_sp_rad");
    assert_eq!(
        diags[1].kind,
        AnomalyKind::Region {
            end_timestamp_us: 4_000_000
        }
    );
    assert!(diags[1].summary.contains("infinity"));
}

#[test]
fn rc_recovery_disarm_missing_and_legacy_flags() {
    let mut a = analyzer("rc_loss");
    arm(a.as_mut());
    feed(a.as_mut(), rc(1_000_000, true, false), 0);
    feed(a.as_mut(), B::new("input_rc").timestamp(1_200_000), 0);
    feed(a.as_mut(), rc(2_000_000, false, false), 0);
    // The legacy uint8 schema remains supported, without accepting invalid 2.
    feed(
        a.as_mut(),
        B::new("input_rc")
            .timestamp(2_500_000)
            .field_u8("rc_lost", 2),
        0,
    );
    feed(
        a.as_mut(),
        B::new("input_rc")
            .timestamp(3_000_000)
            .field_u8("rc_lost", 1),
        0,
    );
    feed(
        a.as_mut(),
        B::new("vehicle_status")
            .timestamp(4_000_000)
            .field_u8("arming_state", 1),
        0,
    );
    let diags = a.finish();
    assert_eq!(diags.len(), 2);
    assert_eq!(
        diags[0].kind,
        AnomalyKind::Region {
            end_timestamp_us: 2_000_000
        }
    );
    assert_eq!(diags[1].timestamp_us, 3_000_000);
    assert_eq!(
        diags[1].kind,
        AnomalyKind::Region {
            end_timestamp_us: 4_000_000
        }
    );
}

#[test]
fn ekf_unavailable_ratios_break_sustained_tracking() {
    for unavailable in [f32::NAN, f32::INFINITY] {
        let mut a = analyzer("ekf_failure");
        for i in 0..=35 {
            feed(
                a.as_mut(),
                ekf(i * 100_000, if i == 18 { unavailable } else { 2.0 }),
                0,
            );
        }
        assert!(a.finish().is_empty());
    }
}

#[test]
fn tecs_does_not_bridge_unobserved_gaps_or_recovered_episodes() {
    let mut a = analyzer("tecs_nonfinite_pitch");
    feed(a.as_mut(), tecs(1_000_000, f32::NAN), 0);
    feed(a.as_mut(), tecs(3_000_000, f32::NAN), 0);
    feed(a.as_mut(), tecs(3_100_000, 0.1), 0);
    feed(a.as_mut(), tecs(3_200_000, f32::NAN), 0);
    feed(a.as_mut(), tecs(3_300_000, f32::NAN), 0);
    let diags = a.finish();
    assert_eq!(diags.len(), 3);
    for (diag, start, end) in [
        (&diags[0], 1_000_000, 1_000_000),
        (&diags[1], 3_000_000, 3_100_000),
        (&diags[2], 3_200_000, 3_300_000),
    ] {
        assert_eq!(diag.timestamp_us, start);
        assert_eq!(
            diag.kind,
            AnomalyKind::Region {
                end_timestamp_us: end
            }
        );
    }
}

#[test]
fn stale_sensor_messages_do_not_fabricate_drops_or_panic() {
    let mut motor = analyzer("motor_failure");
    arm(motor.as_mut());
    feed(motor.as_mut(), output(2_000_000, 1500.0), 0);
    feed(motor.as_mut(), output(1_000_000, 0.0), 0);
    feed(motor.as_mut(), output(2_000_000, 0.0), 0);
    assert!(motor.finish().is_empty());
    let mut pack = analyzer("battery_brownout");
    arm(pack.as_mut());
    feed(pack.as_mut(), battery(40_000_000, 12.0, 4, true), 0);
    feed(pack.as_mut(), battery(1_000_000, 12.0, 4, true), 0);
    assert_eq!(pack.finish().len(), 1);
    let mut a = analyzer("gps_interference");
    for i in 0..10 {
        feed(a.as_mut(), gps(i * 100_000, 1.0, 12, 3), 0);
    }
    feed(a.as_mut(), gps(2_000_000, 20.0, 12, 3), 0);
    feed(a.as_mut(), gps(1_000_000, 20.0, 12, 3), 0);
    assert_eq!(a.finish().len(), 1);
}

#[test]
fn vehicle_status_is_associated_by_timestamp_not_arrival_order() {
    for existing_source in [false, true] {
        let mut a = analyzer("battery_brownout");
        feed(
            a.as_mut(),
            B::new("vehicle_status")
                .timestamp(1_000_000)
                .field_u8("arming_state", 1),
            0,
        );
        if existing_source {
            feed(a.as_mut(), battery(1_500_000, 16.0, 4, true), 0);
        }
        feed(
            a.as_mut(),
            B::new("vehicle_status")
                .timestamp(3_000_000)
                .field_u8("arming_state", 2),
            0,
        );
        feed(a.as_mut(), battery(2_000_000, 12.0, 4, true), 0);
        assert!(
            a.finish().is_empty(),
            "the 2s measurement preceded arming at 3s"
        );
    }
    let mut a = analyzer("motor_failure");
    feed(
        a.as_mut(),
        B::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 2),
        0,
    );
    feed(a.as_mut(), output(1_500_000, 1500.0), 0);
    feed(
        a.as_mut(),
        B::new("vehicle_status")
            .timestamp(3_000_000)
            .field_u8("arming_state", 1),
        0,
    );
    feed(a.as_mut(), output(2_000_000, 0.0), 0);
    let diags = a.finish();
    assert_eq!(
        diags.len(),
        1,
        "the 2s command transition preceded disarming"
    );
    assert_eq!(diags[0].timestamp_us, 2_000_000);
}

#[test]
fn rc_recovery_preserves_pre_loss_signal_evidence() {
    let mut a = analyzer("rc_loss");
    arm(a.as_mut());
    feed(
        a.as_mut(),
        rc(1_000_000, false, false).field_u64("timestamp_last_signal", 900_000),
        0,
    );
    feed(
        a.as_mut(),
        rc(2_000_000, true, false).field_u64("timestamp_last_signal", 900_000),
        0,
    );
    feed(
        a.as_mut(),
        rc(4_000_000, false, false).field_u64("timestamp_last_signal", 4_000_000),
        0,
    );
    let diags = a.finish();
    assert_eq!(diags.len(), 1);
    assert!(matches!(
        diags[0].evidence,
        Evidence::RcLoss {
            last_signal_timestamp_us: 900_000,
            signal_lost_duration_ms: 2000
        }
    ));
}
