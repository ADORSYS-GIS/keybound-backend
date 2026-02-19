use backend_repository::KycSubmissionFilter;

#[test]
fn normalized_clamps_page_and_limit() {
    let filter = KycSubmissionFilter {
        status: Some(" SUBMITTED ".to_owned()),
        search: Some("  alice@example.com  ".to_owned()),
        page: 0,
        limit: 1_000,
    }
    .normalized();

    assert_eq!(filter.page, 1);
    assert_eq!(filter.limit, 100);
    assert_eq!(filter.status.as_deref(), Some("SUBMITTED"));
    assert_eq!(filter.search.as_deref(), Some("alice@example.com"));
}

#[test]
fn normalized_drops_empty_filters() {
    let filter = KycSubmissionFilter {
        status: Some("   ".to_owned()),
        search: Some("\n\t ".to_owned()),
        page: 2,
        limit: 20,
    }
    .normalized();

    assert_eq!(filter.page, 2);
    assert_eq!(filter.limit, 20);
    assert_eq!(filter.status, None);
    assert_eq!(filter.search, None);
}

#[test]
fn offset_uses_page_and_limit() {
    let filter = KycSubmissionFilter {
        status: None,
        search: None,
        page: 3,
        limit: 25,
    }
    .normalized();

    assert_eq!(filter.offset(), 50);
}
