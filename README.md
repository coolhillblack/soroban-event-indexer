# soroban-event-indexer

An embeddable Rust library for indexing Soroban smart contract events.

Stellar RPC's `getEvents` only retains ~7 days of history. Every dApp that
needs longer-lived event data (escrows, payments, ticketing, DeFi) must build
its own ingestion layer — or pay for a hosted indexer. **This crate makes
that a 3-line job.**

```rust
EventIndexer::new(
    IndexerConfig::new("CONTRACT_ID").network(Network::Testnet),
)
.watch(|event| {
    println!("{}: {}", event.ledger, event.event_name());
    Ok(())
})?;
```

## Features

| Feature | What it enables |
|---------|----------------|
| `sqlite` *(default)* | Persist events to a local SQLite database |
| `cli` | The `soroban-indexer` binary |

## Installation

```toml
[dependencies]
soroban-event-indexer = "0.1"
```

To use only the library, without SQLite:

```toml
soroban-event-indexer = { version = "0.1", default-features = false }
```

## Usage

### Configuring via environment variables

`IndexerConfig` can also be built from environment variables, useful for
containerized or CLI deployments:

```rust
let config = IndexerConfig::from_env()?;
```

Recognized variables:

| Variable             | Required | Default     | Description                                  |
|-----------------------|----------|-------------|-----------------------------------------------|
| `CONTRACT_ID`         | yes      | —           | Soroban contract ID to watch                  |
| `STELLAR_NETWORK`     | no       | `testnet`   | `mainnet`, `testnet`, `futurenet`, or a custom RPC URL |
| `START_LEDGER`        | no       | auto-detect | Ledger sequence to start polling from         |
| `POLL_INTERVAL_SECS`  | no       | `6`         | Seconds between polls                         |

### Resuming after a restart

See `examples/resumable_indexer.rs` for a full working example. The short
version: on startup, call `SqliteStorage::latest_ledger()` to get the
ledger sequence of the last event you stored, and pass it to
`IndexerConfig::start_ledger()`. The indexer may re-fetch events from that
ledger, but `soroban_events.id` is UNIQUE and writes use `INSERT OR
IGNORE`, so re-processing is safe — no duplicates, no lost events.

Note: `SqliteStorage::latest_cursor()` returns the RPC paging token rather
than a ledger number, and isn't currently usable for resuming — `getEvents`
pagination doesn't accept a raw ledger as a cursor input, so it can't be
translated back. `latest_ledger()` is the correct method for this purpose.

### Pattern 1 — Watch with a callback

```rust
use soroban_event_indexer::{EventIndexer, IndexerConfig, Network};

fn main() -> anyhow::Result<()> {
    EventIndexer::new(
        IndexerConfig::new("CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC")
            .network(Network::Testnet)
            .start_ledger(199_616),
    )
    .watch(|event| {
        println!("[{}] {} -> {}", event.ledger, event.event_name(), event.value.display());
        Ok(())
    })
}
```

`watch` blocks the calling thread, polling the RPC once per ledger close
(~6s by default) for as long as the process runs.

### Pattern 2 — Run in the background

```rust
let indexer = EventIndexer::new(config);
let stop = indexer.stop_handle();

std::thread::spawn(move || {
    let _ = indexer.watch(|event| {
        // handle event
        Ok(())
    });
});

// from anywhere else in your app:
stop.stop();
```

### Pattern 3 — With SQLite persistence

```rust
use soroban_event_indexer::storage::{sqlite::SqliteStorage, EventStorage};

let db = SqliteStorage::open("events.db")?;
db.migrate()?;

EventIndexer::new(config).watch(move |event| {
    db.save_event(&event)?;
    Ok(())
})?;
```

Restarts are safe — the `id` column is a primary key, so re-inserting an
already-seen event is a silent no-op (`INSERT OR IGNORE`).

### Pattern 4 — Filtered events

```rust
use soroban_event_indexer::EventFilter;

EventIndexer::new(config)
    .with_filter(
        EventFilter::new()
            .topic("transfer")      // only "transfer" events
            .successful_only(),     // skip failed txs
    )
    .watch(handler)?;
```

## CLI

Install the binary:

```bash
cargo install soroban-event-indexer --features cli
```

Watch a contract:

```bash
soroban-indexer watch \
  --contract CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC \
  --network testnet \
  --topic transfer \
  --db events.db
```

## Decoded ScVal types

Topics and event values are decoded from base64 XDR into typed Rust values
without depending on the full `stellar-xdr` crate:

| Soroban type | Rust enum variant |
|---|---|
| `Symbol` | `ScValDecoded::Symbol(String)` |
| `String` | `ScValDecoded::String(String)` |
| `Address` | `ScValDecoded::Address(String)` |
| `U32 / I32 / U64 / I64` | `ScValDecoded::U32(u32)` etc. |
| `U128 / I128` | `ScValDecoded::U128(String)` (string avoids JSON overflow) |
| `Bool` | `ScValDecoded::Bool(bool)` |
| `Bytes` | `ScValDecoded::Bytes(Vec<u8>)` |
| `Vec` | `ScValDecoded::Vec(Vec<ScValDecoded>)` |
| `Map` | `ScValDecoded::Map(Vec<(ScValDecoded, ScValDecoded)>)` |
| `Void` | `ScValDecoded::Void` |
| (decode failure) | `ScValDecoded::Raw(String)` — original base64 preserved |

> **Note on nested Vec/Map:** scalar values decode fully. Deeply nested
> `Vec`/`Map` contents currently report element counts rather than full
> recursive decoding (full XDR recursion needs the `stellar-xdr` crate,
> which this library intentionally avoids to keep compile times and the
> dependency tree small). `raw_value` / `raw_topics` are always preserved
> on every event if you need to decode nested structures yourself.

## Architecture

```
EventIndexer::watch()
  └── polls Stellar RPC (getEvents) every ~6s
        └── decodes base64 XDR ScVal topics + value
              └── applies EventFilter (optional)
                    └── calls your handler, or writes to SqliteStorage
```

## Networks

| `Network` variant | RPC URL |
|---|---|
| `Network::Mainnet` | `https://mainnet.sorobanrpc.com` |
| `Network::Testnet` | `https://soroban-testnet.stellar.org` |
| `Network::Futurenet` | `https://rpc-futurenet.stellar.org` |
| `Network::Custom(url)` | Your own node |

## Contributing

Issues and PRs welcome. This project is part of the Stellar Wave / Drips
ecosystem.

## License

MIT
