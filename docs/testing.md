# Testing

Use `AggregateFixture` for fast aggregate tests without a repository or event
store.

```rust
AggregateFixture::<BankAccount>::new()
    .given(vec![BankAccountEvent::AccountOpened {
        account_id: account_id.clone(),
        owner_name: "Uriah".into(),
    }])
    .when(BankAccountCommand::DepositMoney { amount: 100 })
    .then_expect_events(vec![BankAccountEvent::MoneyDeposited { amount: 100 }])
    .then_expect_revision(1);
```

Use event store contract tests for adapters. A backend should verify:

- Empty streams load as an empty vector.
- `NoStream` succeeds only once per stream.
- `Exact(n)` succeeds only when the current revision is `n`.
- `Any` appends without revision checks.
- Multiple events preserve stream order.
- Metadata is preserved.
- Global sequence is monotonic when supported.
- Concurrent appends cannot corrupt revisions.

The current integration tests in `tests/framework.rs` exercise the in-memory
store contract and repository behavior.
