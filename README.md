# Get some tax info for WA state
Simple lib for getting tax data for addrs in WA State.

First goal is to be simple, with reasonable defaults.

## Example

See the example program
```
cargo run --example get
```

This is what it looks like in code
```rust
// .. in an async environment..

match wataxrate::get("400 Broad St", "Seattle", "98109").await {
    Ok(taxinfo) => println!("Tax rate is {}", taxinfo.rate).
    Err(e) => eprintln!("Error getting tax info: {:?}", e),
}
```

## Gotchas
- Requires `tokio`!!