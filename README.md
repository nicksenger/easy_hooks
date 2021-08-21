# Easy Hooks

This crate is a wrapper around the [topo](https://crates.io/crates/topo) crate which provides a few functions to plug React-style hooks into your application.

1. `use_state` : roots a state variable at this node in the call graph using the provided data function. Currently you may only root _one_ variable per type per node.

```rust
// Returns a state accessor to this position in the callgraph, setting it to 42 if it doesn't yet exist.
let n = use_state(|| 42i32);
```

2. `create_context` : assigns type `T` to the context, returning a handle to `set` / `get` its stored value.

```rust
thread_local! {
    static COUNT: Context<u64> = create_context(42);
}

pub fn main() {
    COUNT.with(|c| c.get()); // 42
    COUNT.with(|c| c.set(12));
    COUNT.with(|c| c.get()); // 12
}
```

3. `sweep` : clears any state or context which was not accessed since the last `sweep`.

```rust
pub fn main() {
    let count = set_count(42);
    set_count(500);
    println!("{}", count.get(|n| *n)); // 42

    easy_hooks::sweep();
    // `count` not accessed between sweeps
    easy_hooks::sweep();

    set_count(500);
    println!("{}", count.get(|n| *n)); // 500
}

fn set_count(n: i32) -> iced_local_state::LocalState<i32> {
    easy_hooks::root(|| use_state(|| n))
}
```


