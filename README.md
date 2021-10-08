# Easy Hooks

This crate is a wrapper around the [topo](https://crates.io/crates/topo) crate which provides a couple functions to plug React-style hooks into your application.

- `use_state` : roots a state variable at this node in the call graph using the provided data function. Currently you may only root _one_ variable per type per node.

```rust
// Returns a state accessor to this position in the callgraph, setting it to 42 if it doesn't yet exist.
let n = use_state(|| 42i32);
```

- `sweep` : clears any state which was not accessed since the last `sweep`.

```rust
pub fn main() {
    let count = set_count(42);
    println!("{}", count.get(|n| *n)); // 42
    let count = set_count(500); // noop because count already set
    println!("{}", count.get(|n| *n)); // still 42

    easy_hooks::sweep();

    println!("{}", count.get(|n| *n)); // still 42

    easy_hooks::sweep();
    // `count` not accessed between sweeps
    easy_hooks::sweep();

    let count = set_count(500);
    println!("{}", count.get(|n| *n)); // 500
}

fn set_count(n: i32) -> LocalState<i32> {
    easy_hooks::root(|| easy_hooks::use_state(|| n))
}
```


