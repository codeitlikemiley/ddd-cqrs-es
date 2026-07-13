use super::*;

#[test]
fn in_memory_store_passes_reusable_contract() {
    assert_event_store_contract::<Counter, _>(
        InMemoryEventStore::<Counter>::new(),
        "contract-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        EventStoreContractOptions::default(),
    );
}

#[test]
fn event_store_contract_accepts_custom_first_sequence() {
    assert_event_store_contract::<Counter, _>(
        OffsetSequenceStore::new(100),
        "offset-contract-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        EventStoreContractOptions::with_expected_first_global_sequence(101),
    );
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_store_passes_reusable_contract() {
    assert_event_store_contract::<Counter, _>(
        ddd_cqrs_es::SqliteEventStore::<Counter>::in_memory().unwrap(),
        "sqlite-contract-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        EventStoreContractOptions::default(),
    );
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_idempotency_store_passes_contract() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let store = ddd_cqrs_es::SqliteIdempotencyStore::new(connection).unwrap();
    assert_sql_idempotency_store_contract(store);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_snapshot_store_persists_latest_snapshot() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let store = ddd_cqrs_es::SqliteSnapshotStore::<Counter>::new(connection).unwrap();
    let counter_id = "sqlite-snapshot-counter".to_owned();
    let older = Counter {
        id: Some(counter_id.clone()),
        value: 1,
        revision: 1,
    };
    let newer = Counter {
        id: Some(counter_id.clone()),
        value: 7,
        revision: 2,
    };

    ddd_cqrs_es::assert_snapshot_store_contract::<Counter, _>(
        store.clone(),
        counter_id.clone(),
        older.clone(),
        newer.clone(),
    );
    store
        .save_snapshot(Snapshot::new(
            counter_id.clone(),
            1,
            older,
            Metadata::default(),
        ))
        .unwrap();

    let loaded = store.load_snapshot(&counter_id).unwrap().unwrap();
    assert_eq!(loaded.revision, 2);
    assert_eq!(loaded.state, newer);
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_store_passes_reusable_contract_when_url_is_provided() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_POSTGRES_URL") else {
        eprintln!("skipping live Postgres contract test: DDD_CQRS_ES_POSTGRES_URL is not set");
        return;
    };
    let table_name = format!(
        "events_live_contract_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    let store = ddd_cqrs_es::PostgresEventStore::<Counter>::connect_with_table_name(
        &database_url,
        table_name,
    )
    .unwrap();
    store.initialize_schema().unwrap();

    assert_event_store_contract::<Counter, _>(
        store,
        "postgres-contract-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        EventStoreContractOptions::default(),
    );
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_idempotency_store_passes_contract_when_url_is_provided() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_POSTGRES_URL") else {
        eprintln!("skipping live Postgres idempotency test: DDD_CQRS_ES_POSTGRES_URL is not set");
        return;
    };
    let table_name = format!(
        "idempotency_live_contract_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    let client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    let store =
        ddd_cqrs_es::PostgresIdempotencyStore::with_table_name(client, table_name.clone()).unwrap();

    assert_sql_idempotency_store_contract(store.clone());
    drop(store);

    let mut cleanup = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    cleanup
        .batch_execute(&format!("DROP TABLE IF EXISTS {table_name};"))
        .unwrap();
}

#[cfg(feature = "mysql")]
#[test]
fn mysql_store_passes_reusable_contract_when_url_is_provided() {
    let _guard = MYSQL_TEST_MUTEX
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(db) = mysql_test_db_or_skip("contract test") else {
        return;
    };
    let table_name = unique_mysql_table("events_live_contract");
    let _cleanup = MySqlTableCleanup::new(&db.test_url, vec![table_name.clone()]);

    let store = ddd_cqrs_es::MySqlEventStore::<Counter>::connect_with_table_name(
        &db.test_url,
        table_name.clone(),
    )
    .unwrap();
    store.initialize_schema().unwrap();

    assert_event_store_contract::<Counter, _>(
        store,
        "mysql-contract-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        EventStoreContractOptions::default(),
    );
}

#[cfg(feature = "mysql")]
#[test]
fn mysql_idempotency_store_passes_contract() {
    let _guard = MYSQL_TEST_MUTEX
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(db) = mysql_test_db_or_skip("idempotency test") else {
        return;
    };
    let table_name = unique_mysql_table("idempotency");
    let _cleanup = MySqlTableCleanup::new(&db.test_url, vec![table_name.clone()]);

    let conn = mysql::Conn::new(db.test_url.as_str()).unwrap();
    let store =
        ddd_cqrs_es::MySqlIdempotencyStore::with_table_name(conn, table_name.clone()).unwrap();

    assert_sql_idempotency_store_contract(store);
}
