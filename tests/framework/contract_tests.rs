use super::*;

const CONTRACT_THIRD_EVENT: CounterEvent = CounterEvent::Incremented { by: 2 };

#[test]
fn in_memory_store_passes_reusable_contract() {
    assert_event_store_contract::<Counter, _>(
        InMemoryEventStore::<Counter>::new(),
        "contract-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        CONTRACT_THIRD_EVENT,
        EventStoreContractOptions::default(),
    );
}

#[test]
fn in_memory_store_passes_any_writer_contract() {
    let store = Arc::new(InMemoryEventStore::<Counter>::new());
    assert_event_store_any_writers_contract::<Counter, _, _>(
        {
            let store = Arc::clone(&store);
            move || (*store).clone()
        },
        "any-writer-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
    );
}

#[test]
fn in_memory_store_passes_append_race_contract() {
    let store = Arc::new(InMemoryEventStore::<Counter>::new());
    assert_event_store_append_race_contract::<Counter, _, _>(
        {
            let store = Arc::clone(&store);
            move || (*store).clone()
        },
        "race-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        8,
    );
}

#[test]
fn in_memory_snapshot_store_passes_contract() {
    use ddd_cqrs_es::{assert_snapshot_store_contract, InMemorySnapshotStore};

    let store = InMemorySnapshotStore::<Counter>::new();
    let counter_id = "memory-snapshot-counter".to_owned();
    let older = Counter {
        id: Some(counter_id.clone()),
        value: 1,
    };
    let newer = Counter {
        id: Some(counter_id.clone()),
        value: 7,
    };

    assert_snapshot_store_contract(store, counter_id, older, newer);
}

#[test]
fn event_store_contract_accepts_custom_first_sequence() {
    assert_event_store_contract::<Counter, _>(
        OffsetSequenceStore::new(100),
        "offset-contract-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        CONTRACT_THIRD_EVENT,
        EventStoreContractOptions::with_expected_first_global_sequence(101),
    );
}

#[cfg(feature = "json-file")]
#[test]
fn json_file_checkpoint_store_passes_contract() {
    let dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let checkpoints_path = dir.join(format!("contract_checkpoints_{nanos}.json"));
    let store = ddd_cqrs_es::JsonFileCheckpointStore::new(checkpoints_path.clone());

    assert_checkpoint_store_contract(store, "json-file-contract-projection");

    let _ = std::fs::remove_file(checkpoints_path);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_store_passes_reusable_contract() {
    assert_event_store_contract::<Counter, _>(
        ddd_cqrs_es::SqliteEventStore::<Counter>::in_memory().unwrap(),
        "sqlite-contract-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        CONTRACT_THIRD_EVENT,
        EventStoreContractOptions::default(),
    );
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_store_passes_any_writer_and_race_contracts() {
    let database_name = format!(
        "file:sqlite_contract_race_{}_{}?mode=memory&cache=shared",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let seed_connection = rusqlite::Connection::open(&database_name).unwrap();
    let seed_store = ddd_cqrs_es::SqliteEventStore::<Counter>::new(seed_connection).unwrap();
    seed_store.initialize_schema().unwrap();

    let make_store = {
        let database_name = database_name.clone();
        move || {
            let connection = rusqlite::Connection::open(&database_name).unwrap();
            ddd_cqrs_es::SqliteEventStore::<Counter>::new(connection).unwrap()
        }
    };

    assert_event_store_any_writers_contract::<Counter, _, _>(
        make_store,
        "sqlite-any-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
    );

    assert_event_store_append_race_contract::<Counter, _, _>(
        {
            let database_name = database_name.clone();
            move || {
                let connection = rusqlite::Connection::open(&database_name).unwrap();
                ddd_cqrs_es::SqliteEventStore::<Counter>::new(connection).unwrap()
            }
        },
        "sqlite-race-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        8,
    );
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_atomic_idempotent_store_passes_contract() {
    let store = ddd_cqrs_es::SqliteEventStore::<Counter>::in_memory().unwrap();
    assert_atomic_idempotent_store_contract::<Counter, _>(
        store,
        "sqlite-atomic-contract-counter".to_owned(),
        IdempotencyKey::new("sqlite-atomic-contract-key"),
        CounterEvent::Created,
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
    };
    let newer = Counter {
        id: Some(counter_id.clone()),
        value: 7,
    };

    ddd_cqrs_es::assert_snapshot_store_contract::<Counter, _>(
        store.clone(),
        counter_id.clone(),
        older.clone(),
        newer.clone(),
    );
    assert!(store
        .save_snapshot(Snapshot::new(
            counter_id.clone(),
            1,
            older,
            Metadata::default(),
        ))
        .is_err());

    let loaded = store.load_snapshot(&counter_id).unwrap().unwrap();
    assert_eq!(loaded.revision, 2);
    assert_eq!(loaded.state, newer);
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_store_passes_reusable_contract_when_url_is_provided() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_POSTGRES_URL") else {
        skip_live_test("Postgres contract test", "DDD_CQRS_ES_POSTGRES_URL");
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
        CONTRACT_THIRD_EVENT,
        EventStoreContractOptions::default(),
    );
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_pooled_store_passes_reusable_contract_when_url_is_provided() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_POSTGRES_URL") else {
        skip_live_test("pooled Postgres contract test", "DDD_CQRS_ES_POSTGRES_URL");
        return;
    };
    let table_name = format!(
        "events_live_pool_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    let store = ddd_cqrs_es::PostgresEventStore::<Counter>::connect_pooled_with_table_name(
        &database_url,
        table_name,
        3,
    )
    .unwrap();
    store.initialize_schema().unwrap();

    assert_event_store_contract::<Counter, _>(
        store,
        "postgres-pooled-contract-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        CONTRACT_THIRD_EVENT,
        EventStoreContractOptions::default(),
    );
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_live_backends_pass_race_and_atomic_contracts_when_url_is_provided() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_POSTGRES_URL") else {
        skip_live_test("Postgres race contract test", "DDD_CQRS_ES_POSTGRES_URL");
        return;
    };
    let table_name = format!(
        "events_live_race_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let database_url = database_url.clone();
    let make_store = {
        let database_url = database_url.clone();
        let table_name = table_name.clone();
        move || {
            let store = ddd_cqrs_es::PostgresEventStore::<Counter>::connect_with_table_name(
                &database_url,
                table_name.clone(),
            )
            .unwrap();
            store.initialize_schema().unwrap();
            store
        }
    };

    assert_event_store_any_writers_contract::<Counter, _, _>(
        make_store,
        "postgres-any-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
    );

    assert_event_store_append_race_contract::<Counter, _, _>(
        {
            let database_url = database_url.clone();
            let table_name = table_name.clone();
            move || {
                let store = ddd_cqrs_es::PostgresEventStore::<Counter>::connect_with_table_name(
                    &database_url,
                    table_name.clone(),
                )
                .unwrap();
                store.initialize_schema().unwrap();
                store
            }
        },
        "postgres-race-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        6,
    );

    let atomic_table = format!("{table_name}_atomic");
    let idempotency_table = format!("{table_name}_idem");
    let atomic_client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    let atomic_store = ddd_cqrs_es::PostgresEventStore::<Counter>::with_table_names(
        atomic_client,
        atomic_table,
        idempotency_table,
    )
    .unwrap();
    atomic_store.initialize_schema().unwrap();
    assert_atomic_idempotent_store_contract::<Counter, _>(
        atomic_store,
        "postgres-atomic-contract-counter".to_owned(),
        IdempotencyKey::new(format!(
            "postgres-atomic-contract-key-{}",
            std::process::id()
        )),
        CounterEvent::Created,
    );

    let snapshot_client = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
    let snapshot_store = ddd_cqrs_es::PostgresSnapshotStore::<Counter>::with_table_name(
        snapshot_client,
        format!("{table_name}_snapshots"),
    )
    .unwrap();
    let counter_id = "postgres-snapshot-contract-counter".to_owned();
    let older = Counter {
        id: Some(counter_id.clone()),
        value: 1,
    };
    let newer = Counter {
        id: Some(counter_id.clone()),
        value: 7,
    };
    ddd_cqrs_es::assert_snapshot_store_contract(snapshot_store, counter_id, older, newer);
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_idempotency_store_passes_contract_when_url_is_provided() {
    let Ok(database_url) = std::env::var("DDD_CQRS_ES_POSTGRES_URL") else {
        skip_live_test("Postgres idempotency test", "DDD_CQRS_ES_POSTGRES_URL");
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
        CONTRACT_THIRD_EVENT,
        EventStoreContractOptions::default(),
    );
}

#[cfg(feature = "mysql")]
#[test]
fn mysql_pooled_store_passes_reusable_contract_when_url_is_provided() {
    let _guard = MYSQL_TEST_MUTEX
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(db) = mysql_test_db_or_skip("pooled contract test") else {
        return;
    };
    let table_name = unique_mysql_table("events_live_pool");
    let _cleanup = MySqlTableCleanup::new(&db.test_url, vec![table_name.clone()]);

    let store = ddd_cqrs_es::MySqlEventStore::<Counter>::connect_pooled_with_table_name(
        &db.test_url,
        table_name,
        3,
    )
    .unwrap();
    store.initialize_schema().unwrap();

    assert_event_store_contract::<Counter, _>(
        store,
        "mysql-pooled-contract-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        CONTRACT_THIRD_EVENT,
        EventStoreContractOptions::default(),
    );
}

#[cfg(feature = "mysql")]
#[test]
fn mysql_live_backends_pass_race_and_atomic_contracts() {
    let _guard = MYSQL_TEST_MUTEX
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(db) = mysql_test_db_or_skip("race contract test") else {
        return;
    };
    let table_name = unique_mysql_table("events_live_race");
    let _cleanup = MySqlTableCleanup::new(&db.test_url, vec![table_name.clone()]);
    let test_url = db.test_url.clone();

    let make_store = {
        let test_url = test_url.clone();
        let table_name = table_name.clone();
        move || {
            let store = ddd_cqrs_es::MySqlEventStore::<Counter>::connect_with_table_name(
                &test_url,
                table_name.clone(),
            )
            .unwrap();
            store.initialize_schema().unwrap();
            store
        }
    };

    assert_event_store_any_writers_contract::<Counter, _, _>(
        make_store,
        "mysql-any-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
    );

    assert_event_store_append_race_contract::<Counter, _, _>(
        {
            let test_url = test_url.clone();
            let table_name = table_name.clone();
            move || {
                let store = ddd_cqrs_es::MySqlEventStore::<Counter>::connect_with_table_name(
                    &test_url,
                    table_name.clone(),
                )
                .unwrap();
                store.initialize_schema().unwrap();
                store
            }
        },
        "mysql-race-counter".to_owned(),
        CounterEvent::Created,
        CounterEvent::Incremented { by: 1 },
        6,
    );

    let atomic_table = unique_mysql_table("events_live_atomic");
    let idempotency_table = unique_mysql_table("idempotency_atomic");
    let atomic_conn = mysql::Conn::new(test_url.as_str()).unwrap();
    let atomic_store = ddd_cqrs_es::MySqlEventStore::<Counter>::with_table_names(
        atomic_conn,
        atomic_table,
        idempotency_table,
    )
    .unwrap();
    atomic_store.initialize_schema().unwrap();
    assert_atomic_idempotent_store_contract::<Counter, _>(
        atomic_store,
        "mysql-atomic-contract-counter".to_owned(),
        IdempotencyKey::new(format!("mysql-atomic-contract-key-{}", std::process::id())),
        CounterEvent::Created,
    );

    let snapshot_conn = mysql::Conn::new(test_url.as_str()).unwrap();
    let snapshot_store = ddd_cqrs_es::MySqlSnapshotStore::<Counter>::with_table_name(
        snapshot_conn,
        unique_mysql_table("snapshots_contract"),
    )
    .unwrap();
    let counter_id = "mysql-snapshot-contract-counter".to_owned();
    let older = Counter {
        id: Some(counter_id.clone()),
        value: 1,
    };
    let newer = Counter {
        id: Some(counter_id.clone()),
        value: 7,
    };
    ddd_cqrs_es::assert_snapshot_store_contract(snapshot_store, counter_id, older, newer);
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
