use token_pulse_core::{
    error::ErrorCode,
    numeric::EpochMs,
    protocol::{DateRange, DimensionSelection, UsageFilter},
    query::{FacetDimension, FilterOptionsQuery, FilterOptionsRequest},
};
fn query(dimension: FacetDimension) -> FilterOptionsQuery {
    let selected = |id: &str| DimensionSelection::Ids {
        ids: vec![id.into()],
        include_unknown: true,
    };
    FilterOptionsQuery {
        filter: UsageFilter {
            range: DateRange {
                start_ms: EpochMs::new(1000).unwrap(),
                end_ms: EpochMs::new(2000).unwrap(),
                timezone: "Asia/Shanghai".into(),
            },
            sources: selected("source"),
            models: selected("model"),
            projects: selected("project"),
            sessions: selected("session"),
        },
        dimension,
        search: "未知".into(),
        page_size: 200,
    }
}
#[test]
fn each_facet_ignores_only_its_own_selection_and_preserves_the_original_scope() {
    for (dimension, index) in [
        (FacetDimension::Sources, 0),
        (FacetDimension::Models, 1),
        (FacetDimension::Projects, 2),
        (FacetDimension::Sessions, 3),
    ] {
        let q = query(dimension);
        let scope = q.facet_filter().unwrap();
        assert_eq!(scope.range.start_ms.value(), 1000);
        assert_eq!(scope.range.end_ms.value(), 2000);
        assert_eq!(scope.range.timezone, "Asia/Shanghai");
        let before = [
            &q.filter.sources,
            &q.filter.models,
            &q.filter.projects,
            &q.filter.sessions,
        ];
        let after = [
            &scope.sources,
            &scope.models,
            &scope.projects,
            &scope.sessions,
        ];
        for i in 0..4 {
            if i == index {
                assert!(matches!(after[i], DimensionSelection::All {}));
                assert!(matches!(
                    before[i],
                    DimensionSelection::Ids {
                        include_unknown: true,
                        ..
                    }
                ));
            } else {
                assert_eq!(
                    serde_json::to_value(after[i]).unwrap(),
                    serde_json::to_value(before[i]).unwrap()
                );
            }
        }
    }
}
#[test]
fn facet_request_bounds_reject_invalid_search_ranges_and_cursor_shapes() {
    let q = query(FacetDimension::Models);
    assert!(
        FilterOptionsRequest {
            query: q.clone(),
            cursor: None
        }
        .validate()
        .is_ok()
    );
    for size in [0, 201, u16::MAX] {
        let mut bad = q.clone();
        bad.page_size = size;
        assert_eq!(bad.validate().unwrap_err(), ErrorCode::InvalidQuery);
    }
    for search in [
        "a".repeat(257),
        "中".repeat(257),
        "text\nquery".into(),
        "\0".into(),
    ] {
        let mut bad = q.clone();
        bad.search = search;
        assert_eq!(bad.validate().unwrap_err(), ErrorCode::InvalidQuery);
    }
    let mut boundary = q.clone();
    boundary.search = "a".repeat(256);
    boundary.page_size = 1;
    assert!(boundary.validate().is_ok());
    boundary.search = "中".repeat(256);
    assert!(boundary.validate().is_ok());
    let mut bad_range = q.clone();
    bad_range.filter.range.end_ms = bad_range.filter.range.start_ms;
    assert_eq!(
        bad_range.facet_filter().unwrap_err(),
        ErrorCode::InvalidQuery
    );
    for cursor in [
        String::new(),
        "a".repeat(150),
        "a".repeat(152),
        "!".repeat(151),
        format!("{}=", "a".repeat(150)),
    ] {
        assert_eq!(
            FilterOptionsRequest {
                query: q.clone(),
                cursor: Some(cursor)
            }
            .validate()
            .unwrap_err(),
            ErrorCode::CursorInvalid
        );
    }
    // Shape alone is insufficient; signature and live lease are checked by Store.
    assert!(
        FilterOptionsRequest {
            query: q,
            cursor: Some("a".repeat(151))
        }
        .validate()
        .is_ok()
    );
}
