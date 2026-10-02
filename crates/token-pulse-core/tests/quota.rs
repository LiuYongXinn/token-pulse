//! Synthetic wire objects and literal independent expectations. Never uses account credentials.
use serde_json::{Value, json};
use token_pulse_core::{error::ErrorCode, numeric::EpochMs, protocol::QuotaState, quota::*};
fn time(mono: u64) -> QuotaTime {
    QuotaTime {
        monotonic_ms: mono,
        wall: EpochMs::new(1_000_000 + mono as i64).unwrap(),
    }
}
fn book(used: f64) -> QuotaBook {
    parse_quota_read(&json!({"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":used,"windowDurationMins":15,"resetsAt":1234}}}})).unwrap()
}
fn request(cache: &mut QuotaCoordinator, at: u64) -> QuotaReadToken {
    match cache.begin_refresh(time(at), true, true).unwrap() {
        QuotaRefreshDecision::Started(token) => token,
        result => panic!("expected request: {result:?}"),
    }
}
fn connected() -> QuotaCoordinator {
    let mut cache = QuotaCoordinator::new("fixture").unwrap();
    let epoch = cache.begin_connection(time(0)).unwrap();
    cache
        .account_result(&epoch, AccountAvailability::QuotaEligible, time(0))
        .unwrap();
    cache
}
fn ready() -> QuotaCoordinator {
    let mut cache = connected();
    let token = request(&mut cache, 0);
    assert_eq!(
        cache.read_succeeded(&token, book(25.0), time(100)).unwrap(),
        QuotaReplyOutcome::Published
    );
    cache
}
#[test]
fn multibucket_precedence_actual_durations_and_seconds_preserve_unknown_and_true_zero() {
    let parsed=parse_quota_read(&json!({"rateLimits":{"limitId":"old","primary":{"usedPercent":1}},"rateLimitsByLimitId":{"codex":{"limitId":"codex","limitName":"Codex","primary":{"usedPercent":100,"windowDurationMins":10080,"resetsAt":1730947200},"secondary":{"usedPercent":25.5,"windowDurationMins":15,"resetsAt":null},"credits":{"balance":"secret-not-kept"}},"other":{"primary":{"usedPercent":null}}}})).unwrap();
    assert_eq!(parsed.len(), 2);
    assert!(!parsed.contains_key("old"));
    let windows = &parsed["codex"].windows;
    assert_eq!(weekly_window(windows).unwrap().window_id, "primary");
    assert_eq!(shortest_window(windows).unwrap().window_id, "secondary");
    assert_eq!(windows[0].remaining_percent, Some(0.0));
    assert_eq!(windows[0].resets_at_ms.unwrap().value(), 1_730_947_200_000);
    assert_eq!(windows[1].remaining_percent, Some(74.5));
    assert_eq!(windows[1].resets_at_ms, None);
    assert_eq!(parsed["other"].windows[0].used_percent, None);
    assert_eq!(parsed["other"].windows[0].remaining_percent, None);
    let mut cache = connected();
    let token = request(&mut cache, 0);
    cache.read_succeeded(&token, parsed, time(1)).unwrap();
    let snapshot = serde_json::to_value(cache.snapshot()).unwrap();
    assert_eq!(snapshot["selected_limit_id"], "codex");
    assert!(!snapshot.to_string().contains("secret-not-kept"));
}
#[test]
fn invalid_window_values_remain_null_without_coercion_and_overusage_clamps() {
    let parsed=parse_quota_read(&json!({"rateLimits":{"primary":{"usedPercent":"0","windowDurationMins":0,"resetsAt":9223372036854775807_i64},"secondary":{"usedPercent":150,"windowDurationMins":-1,"resetsAt":-1}}})).unwrap();
    let windows = &parsed[LEGACY_LIMIT_ID].windows;
    assert_eq!(
        (
            windows[0].used_percent,
            windows[0].remaining_percent,
            windows[0].duration_mins,
            windows[0].resets_at_ms
        ),
        (None, None, None, None)
    );
    assert_eq!(windows[1].remaining_percent, Some(0.0));
    assert_eq!(windows[1].duration_mins, None);
    assert_eq!(windows[1].resets_at_ms.unwrap().value(), -1000);
    let parsed = parse_quota_read(
        &json!({"rateLimits":{"primary":{"usedPercent":-2,"windowDurationMins":4294967296_u64}}}),
    )
    .unwrap();
    assert_eq!(
        parsed[LEGACY_LIMIT_ID].windows[0].remaining_percent,
        Some(100.0)
    );
    assert_eq!(parsed[LEGACY_LIMIT_ID].windows[0].duration_mins, None);
}
#[test]
fn authoritative_empty_map_legacy_fallback_and_malformed_bucket_identity_are_distinct() {
    assert!(
        parse_quota_read(
            &json!({"rateLimitsByLimitId":{},"rateLimits":{"primary":{"usedPercent":90}}})
        )
        .unwrap()
        .is_empty()
    );
    assert!(parse_quota_read(&json!({"rateLimitsByLimitId":null,"rateLimits":{"secondary":{"usedPercent":0,"windowDurationMins":10080}}})).unwrap()[LEGACY_LIMIT_ID].legacy_identity);
    for value in [
        json!({}),
        json!({"rateLimitsByLimitId":[]}),
        json!({"rateLimitsByLimitId":{"codex":{"limitId":"other"}}}),
        json!({"rateLimits":{"primary":true}}),
        json!({"rateLimits":{"limitId":"\n"}}),
    ] {
        assert!(parse_quota_read(&value).is_err());
    }
    let many: serde_json::Map<String, Value> = (0..129)
        .map(|i| (format!("bucket-{i}"), json!({})))
        .collect();
    assert_eq!(
        parse_quota_read(&json!({"rateLimitsByLimitId":many})).unwrap_err(),
        ErrorCode::QuotaProtocolError
    );
}
#[test]
fn account_detection_does_not_turn_api_key_or_an_unknown_provider_into_quota() {
    assert_eq!(
        account_availability(&json!({"requiresOpenaiAuth":true,"account":null})).unwrap(),
        AccountAvailability::AuthorizationRequired
    );
    assert_eq!(
        account_availability(&json!({"requiresOpenaiAuth":false,"account":null})).unwrap(),
        AccountAvailability::Unsupported
    );
    for kind in ["apiKey", "amazonBedrock", "futureAuth"] {
        assert_eq!(
            account_availability(&json!({"requiresOpenaiAuth":true,"account":{"type":kind}}))
                .unwrap(),
            AccountAvailability::Unsupported
        );
    }
    assert_eq!(account_availability(&json!({"requiresOpenaiAuth":true,"account":{"type":"chatgpt","planType":"pro","email":"synthetic@example.test"}})).unwrap(),AccountAvailability::QuotaEligible);
    assert_eq!(
        account_availability(&json!({"account":null})),
        Err(ErrorCode::QuotaProtocolError)
    );
}
#[test]
fn disconnected_and_identity_switch_clear_all_values_and_reject_previous_epoch_and_unproven_notifications()
 {
    let mut cache = ready();
    let old = cache.epoch();
    let pending = request(&mut cache, 5100);
    let new = cache.account_changed(time(5200)).unwrap();
    assert_ne!(old, new);
    let snapshot = cache.snapshot();
    assert!(snapshot.windows.is_empty());
    assert!(snapshot.available_limits.is_empty());
    assert_eq!(snapshot.fetched_at_ms, None);
    assert_eq!(snapshot.selected_limit_id, None);
    assert_eq!(
        cache
            .read_succeeded(&pending, book(2.0), time(5300))
            .unwrap(),
        QuotaReplyOutcome::Ignored
    );
    assert!(
        !cache
            .account_result(&old, AccountAvailability::QuotaEligible, time(5300))
            .unwrap()
    );
    cache
        .account_result(&new, AccountAvailability::QuotaEligible, time(5300))
        .unwrap();
    assert!(
        !cache
            .notification(
                &new,
                parse_quota_update(
                    &json!({"rateLimits":{"limitId":"codex","primary":{"usedPercent":1}}})
                )
                .unwrap(),
                time(5400)
            )
            .unwrap()
    );
    cache.disconnect(time(5500)).unwrap();
    let disconnected = cache.snapshot();
    assert!(matches!(disconnected.state, QuotaState::Disconnected));
    assert_eq!(disconnected.error_code, None);
    assert!(
        !cache
            .account_result(
                &cache.epoch(),
                AccountAvailability::QuotaEligible,
                time(5600)
            )
            .unwrap()
    );
    assert_eq!(
        cache.begin_refresh(time(5600), true, true).unwrap_err(),
        ErrorCode::QuotaDisconnected
    );
}
#[test]
fn single_flight_minimum_interval_visible_and_hidden_polling_use_monotonic_time() {
    let mut cache = connected();
    let token = request(&mut cache, 0);
    assert_eq!(token.request_id, "quota-fixture-1-read-1");
    assert_eq!(
        cache.begin_refresh(time(1), true, true).unwrap(),
        QuotaRefreshDecision::InFlight
    );
    cache.read_succeeded(&token, book(25.0), time(100)).unwrap();
    assert_eq!(
        cache.begin_refresh(time(5099), true, true).unwrap(),
        QuotaRefreshDecision::RateLimited { retry_after_ms: 1 }
    );
    assert_eq!(
        cache.begin_refresh(time(60099), false, true).unwrap(),
        QuotaRefreshDecision::NotDue
    );
    let token = match cache.begin_refresh(time(60100), false, true).unwrap() {
        QuotaRefreshDecision::Started(token) => token,
        _ => panic!("visible interval"),
    };
    cache
        .read_succeeded(&token, book(25.0), time(60200))
        .unwrap();
    assert_eq!(
        cache.begin_refresh(time(360199), false, false).unwrap(),
        QuotaRefreshDecision::NotDue
    );
    assert!(matches!(
        cache
            .begin_refresh(
                QuotaTime {
                    wall: EpochMs::new(-50).unwrap(),
                    monotonic_ms: 360200
                },
                false,
                false
            )
            .unwrap(),
        QuotaRefreshDecision::Started(_)
    ));
}
#[test]
fn backoff_uses_literal_5_15_30_60_delays_and_keeps_old_success_and_percentages() {
    let mut cache = ready();
    let mut now = 5100;
    for delay in [5000, 15000, 30000, 60000, 60000] {
        let token = request(&mut cache, now);
        cache
            .read_failed(&token, ErrorCode::QuotaServiceUnavailable, time(now + 1))
            .unwrap();
        let snapshot = cache.snapshot();
        assert!(matches!(snapshot.state, QuotaState::Stale));
        assert_eq!(snapshot.fetched_at_ms.unwrap().value(), 1000100);
        assert_eq!(snapshot.windows[0].remaining_percent, Some(75.0));
        assert_eq!(
            cache.begin_refresh(time(now + 1), true, true).unwrap(),
            QuotaRefreshDecision::RateLimited {
                retry_after_ms: delay
            }
        );
        now += delay + 1;
    }
    let token = request(&mut cache, now);
    cache
        .read_succeeded(&token, book(50.0), time(now + 1))
        .unwrap();
    assert!(matches!(cache.snapshot().state, QuotaState::Ready));
    assert_eq!(cache.snapshot().error_code, None);
}
#[test]
fn timeout_and_late_reply_are_rejected_even_without_a_prior_timer_tick() {
    let mut cache = connected();
    let token = request(&mut cache, 0);
    assert_eq!(
        cache
            .read_succeeded(&token, book(1.0), time(10000))
            .unwrap(),
        QuotaReplyOutcome::Expired
    );
    assert_eq!(
        cache.snapshot().error_code.as_deref(),
        Some("QUOTA_TIMEOUT")
    );
    assert!(cache.snapshot().windows.is_empty());
    assert_eq!(
        cache
            .read_succeeded(&token, book(1.0), time(10001))
            .unwrap(),
        QuotaReplyOutcome::Ignored
    );
    let token = request(&mut cache, 15000);
    assert!(cache.tick(time(25000)).unwrap());
    assert_eq!(
        cache
            .read_succeeded(&token, book(2.0), time(25001))
            .unwrap(),
        QuotaReplyOutcome::Ignored
    );
}
#[test]
fn notifications_keep_other_buckets_timestamps_and_overtake_an_older_read_reply() {
    let mut cache = connected();
    let token = request(&mut cache, 0);
    let all=parse_quota_read(&json!({"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":25}},"other":{"primary":{"usedPercent":50}}}})).unwrap();
    cache.read_succeeded(&token, all, time(100)).unwrap();
    let token = request(&mut cache, 5100);
    let epoch = cache.epoch();
    cache
        .notification(
            &epoch,
            parse_quota_update(
                &json!({"rateLimits":{"limitId":"codex","primary":{"usedPercent":80}}}),
            )
            .unwrap(),
            time(5200),
        )
        .unwrap();
    cache.read_succeeded(&token, book(2.0), time(5300)).unwrap();
    let snapshot = cache.snapshot();
    assert_eq!(snapshot.windows[0].remaining_percent, Some(20.0));
    assert_eq!(snapshot.fetched_at_ms.unwrap().value(), 1005200);
    cache
        .notification(
            &epoch,
            parse_quota_update(
                &json!({"rateLimits":{"limitId":"other","primary":{"usedPercent":5}}}),
            )
            .unwrap(),
            time(5400),
        )
        .unwrap();
    assert_eq!(cache.snapshot().fetched_at_ms.unwrap().value(), 1005200);
    cache.select_limit("other").unwrap();
    assert_eq!(cache.snapshot().windows[0].remaining_percent, Some(95.0));
    assert_eq!(cache.snapshot().fetched_at_ms.unwrap().value(), 1005400);
}
#[test]
fn ambiguous_selection_or_unidentified_notification_never_combines_buckets() {
    let mut cache = connected();
    let token = request(&mut cache, 0);
    let all=parse_quota_read(&json!({"rateLimitsByLimitId":{"a":{"primary":{"usedPercent":10}},"b":{"primary":{"usedPercent":20}}}})).unwrap();
    cache.read_succeeded(&token, all, time(1)).unwrap();
    assert_eq!(cache.snapshot().selected_limit_id, None);
    assert!(cache.snapshot().windows.is_empty());
    let epoch = cache.epoch();
    assert!(
        !cache
            .notification(
                &epoch,
                parse_quota_update(&json!({"rateLimits":{"primary":{"usedPercent":0}}})).unwrap(),
                time(2)
            )
            .unwrap()
    );
    cache.select_limit("b").unwrap();
    assert_eq!(cache.snapshot().windows[0].remaining_percent, Some(80.0));
    cache
        .notification(
            &epoch,
            parse_quota_update(
                &json!({"rateLimitsByLimitId":{"a":{"primary":{"usedPercent":50}}}}),
            )
            .unwrap(),
            time(3),
        )
        .unwrap();
    assert_eq!(cache.snapshot().selected_limit_id, None);
    assert!(cache.snapshot().windows.is_empty());
    assert_eq!(cache.select_limit("missing"), Err(ErrorCode::InvalidQuery));
}
#[test]
fn stale_clock_does_not_refill_expired_windows_or_change_their_reset_time() {
    let mut cache = ready();
    assert!(!cache.tick(time(300099)).unwrap());
    assert!(cache.tick(time(300100)).unwrap());
    let snapshot = cache.snapshot();
    assert!(matches!(snapshot.state, QuotaState::Stale));
    assert_eq!(snapshot.windows[0].remaining_percent, Some(75.0));
    assert_eq!(snapshot.windows[0].resets_at_ms.unwrap().value(), 1234000);
    let revision = snapshot.quota_revision;
    assert!(!cache.tick(time(300101)).unwrap());
    assert_eq!(cache.snapshot().quota_revision, revision);
}
#[test]
fn legacy_notifications_only_update_a_proven_legacy_identity_and_weekly_ambiguity_stays_unknown() {
    let mut cache = connected();
    let token = request(&mut cache, 0);
    let value = json!({"rateLimits":{"primary":{"usedPercent":10,"windowDurationMins":10080},"secondary":{"usedPercent":90,"windowDurationMins":10080}}});
    let legacy = parse_quota_read(&value).unwrap();
    assert!(weekly_window(&legacy[LEGACY_LIMIT_ID].windows).is_none());
    assert!(shortest_window(&legacy[LEGACY_LIMIT_ID].windows).is_none());
    cache.read_succeeded(&token, legacy, time(1)).unwrap();
    let epoch = cache.epoch();
    assert!(
        cache
            .notification(
                &epoch,
                parse_quota_update(&json!({"rateLimits":{"primary":{"usedPercent":70}}})).unwrap(),
                time(2)
            )
            .unwrap()
    );
    assert_eq!(cache.snapshot().windows[0].remaining_percent, Some(30.0));
    let epoch = cache.begin_connection(time(3)).unwrap();
    cache
        .account_result(&epoch, AccountAvailability::QuotaEligible, time(3))
        .unwrap();
    let token = request(&mut cache, 3);
    let named = parse_quota_read(
        &json!({"rateLimitsByLimitId":{"legacy":{"limitName":17,"primary":{"usedPercent":25}}}}),
    )
    .unwrap();
    assert_eq!(named["legacy"].limit.display_name, None);
    cache.read_succeeded(&token, named, time(4)).unwrap();
    assert!(
        !cache
            .notification(
                &epoch,
                parse_quota_update(&json!({"rateLimits":{"primary":{"usedPercent":70}}})).unwrap(),
                time(5)
            )
            .unwrap()
    );
    assert_eq!(cache.snapshot().windows[0].remaining_percent, Some(75.0));
}

#[test]
fn transport_failures_preserve_only_proven_stale_values_and_new_connection_clears_them() {
    let mut cache = connected();
    let token = request(&mut cache, 0);
    cache
        .read_succeeded(
            &token,
            parse_quota_read(
                &json!({"rateLimits":{"limitId":"codex", "primary":{"usedPercent":25}}}),
            )
            .unwrap(),
            time(1),
        )
        .unwrap();
    let epoch = cache.epoch();
    assert!(
        !cache
            .connection_failed("old", ErrorCode::QuotaTimeout, time(2))
            .unwrap()
    );
    assert_eq!(
        cache.connection_failed(&epoch, ErrorCode::DbWriteFailed, time(2)),
        Err(ErrorCode::InvalidQuery)
    );
    assert!(
        cache
            .connection_failed(&epoch, ErrorCode::QuotaServiceUnavailable, time(2))
            .unwrap()
    );
    let snapshot = cache.snapshot();
    assert!(matches!(snapshot.state, QuotaState::Stale));
    assert_eq!(snapshot.windows[0].remaining_percent, Some(75.0));
    assert_eq!(snapshot.fetched_at_ms, Some(time(1).wall));
    assert_eq!(
        snapshot.error_code.as_deref(),
        Some("QUOTA_SERVICE_UNAVAILABLE")
    );
    assert!(matches!(
        cache.begin_refresh(time(3), true, true),
        Err(ErrorCode::QuotaDisconnected)
    ));
    let next = cache.begin_connection(time(4)).unwrap();
    assert_ne!(next, epoch);
    assert!(cache.snapshot().windows.is_empty());
    assert_eq!(cache.snapshot().fetched_at_ms, None);
    cache
        .connection_failed(&next, ErrorCode::QuotaUnsupported, time(5))
        .unwrap();
    assert!(matches!(cache.snapshot().state, QuotaState::Unsupported));
    cache.disconnect(time(6)).unwrap();
    let disconnected = cache.epoch();
    assert!(
        !cache
            .connection_failed(&disconnected, ErrorCode::QuotaTimeout, time(7))
            .unwrap()
    );
}

#[test]
fn refresh_result_keeps_exact_control_state_and_latest_shared_privacy() {
    use token_pulse_core::{
        numeric::DecimalInt,
        privacy::{DisplayPolicyStamp, PrivacyState, PrivateResponse},
    };
    let mut cache = connected();
    let token = request(&mut cache, 0);
    cache.read_succeeded(&token, parse_quota_read(&json!({"rateLimits":{"limitId":"codex","limitName":"PRIVATE NAME","primary":{"usedPercent":100}}})).unwrap(), time(1)).unwrap();
    let result = QuotaRefreshResult::from_decision(
        QuotaRefreshDecision::RateLimited {
            retry_after_ms: 4999,
        },
        cache.snapshot(),
    )
    .unwrap();
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["status"], "rate_limited");
    assert_eq!(json["retry_after_ms"], 4999);
    assert_eq!(json["quota"]["windows"][0]["remaining_percent"], 0.0);
    let privacy = PrivacyState::new(DisplayPolicyStamp {
        settings_revision: DecimalInt::parse("1").unwrap(),
        privacy: false,
    });
    let response = PrivateResponse::new("quota-fixture".into(), result.clone(), privacy.clone());
    privacy
        .publish(DisplayPolicyStamp {
            settings_revision: DecimalInt::parse("2").unwrap(),
            privacy: true,
        })
        .unwrap();
    let hidden = serde_json::to_value(response).unwrap();
    assert!(!hidden.to_string().contains("PRIVATE NAME"));
    assert_eq!(
        hidden["data"]["quota"]["windows"][0]["remaining_percent"],
        0.0
    );
    assert_eq!(
        result.quota.available_limits[0].display_name.as_deref(),
        Some("PRIVATE NAME")
    );
    assert!(
        QuotaRefreshResult::from_decision(
            QuotaRefreshDecision::RateLimited {
                retry_after_ms: u64::MAX
            },
            cache.snapshot()
        )
        .is_err()
    );
    let mut extra = json;
    extra["credentials"] = serde_json::json!("SECRET");
    assert!(serde_json::from_value::<QuotaRefreshResult>(extra).is_err());
    let event = serde_json::to_value(QuotaChanged::from(&cache.snapshot())).unwrap();
    assert_eq!(event.as_object().unwrap().len(), 3);
    assert!(!event.to_string().contains("PRIVATE NAME"));
}
