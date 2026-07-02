---
title: Reactive Leptos UI
description: Build the polished optimistic Leptos UI and explain hydration states for the event-sourced counter app.
---

# Reactive Leptos UI

## 6. Polished Premium UI Walkthrough

On the frontend, Leptos uses reactive signals, server actions, and direct button dispatch to provide a snappy, fluid user interface. The current counter app keeps a local optimistic count, tracks the sequence it expects the server to reach, and ignores older action/SSE snapshots so rapid clicks do not visibly rewind the number:

```rust
#[component]
fn HomePage() -> impl IntoView {
    let increment_action = ServerAction::<IncrementCount>::new();
    let counter_view = Resource::new(|| (), |_| get_counter_view());
    let (current_view, set_current_view) = signal(None::<CounterViewDto>);
    let (optimistic_count, set_optimistic_count) = signal(None::<i32>);
    let (last_seen_sequence, set_last_seen_sequence) = signal(0_u64);
    let (pending_until_sequence, set_pending_until_sequence) = signal(None::<u64>);

    Effect::new(move |_| {
        if let Some(Ok(view_data)) = counter_view.get() {
            if current_view
                .get_untracked()
                .is_some_and(|current| view_data.last_sequence < current.last_sequence)
            {
                return;
            }

            let caught_up = pending_until_sequence
                .get_untracked()
                .is_none_or(|sequence| view_data.last_sequence >= sequence);

            if caught_up {
                set_optimistic_count.set(Some(view_data.count));
                set_pending_until_sequence.set(None);
            }
            set_last_seen_sequence.set(view_data.last_sequence);
            set_current_view.set(Some(view_data));
        }
    });

    let apply_optimistic_increment = move |_| {
        let base_count = optimistic_count
            .get_untracked()
            .or_else(|| current_view.get_untracked().map(|view| view.count))
            .unwrap_or_default();
        set_optimistic_count.set(Some(base_count.saturating_add(1)));

        let base_sequence = pending_until_sequence
            .get_untracked()
            .or_else(|| current_view.get_untracked().map(|view| view.last_sequence))
            .unwrap_or_else(|| last_seen_sequence.get_untracked());
        set_pending_until_sequence.set(Some(base_sequence.saturating_add(1)));

        increment_action.dispatch(IncrementCount { amount: 1 });
    };

    view! {
        <button on:click=apply_optimistic_increment>
            "+1"
        </button>
        <div>{move || optimistic_count.get().unwrap_or_default()}</div>
    }
}
```

### Hydration Mechanics & UI States
1.  **Server-Side Rendering (SSR)**: When the page is loaded, the server triggers `get_counter_view()`, renders the HTML layout with the true count and latest ledger entries, and sends down static markup. The user sees a fully rendered page instantly.
2.  **Hydration**: The compiled Client WebAssembly binary is loaded by the browser, intercepts the static page, attaches event listeners, and initializes signals. The transition is completely invisible and painless.
3.  **Optimistic State Updates**: On button click, the UI applies the local count change immediately and dispatches the server action in the background. The client only lets authoritative responses replace the optimistic count once the returned sequence catches up to the expected sequence, so older SSE/action responses cannot move the visible value backward during bursty clicks.

---

