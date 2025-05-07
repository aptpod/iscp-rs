# iSCP-rs

iSCPv2 Client Library

## Installation

Add this dependency to your Cargo.toml:

```toml
iscp-rs = "1"
```

## Detailed Documentation

- [API Documentation](https://docs.rs/iscp-rs/latest/iscp/)
- [Example Code](./examples)

### Crate Features

* `gen`: Regenerates protobuf messages for using the latest protobuf version.
* `unstable-webtransport`: Enables experimental WebTransport support. To use this feature, add the following to your Cargo.toml:

  ```toml
  iscp-rs = { version = "1", features = ["unstable-webtransport"] }
  ```

  This feature adds WebTransport as an alternative transport protocol alongside WebSockets. Note that this is an experimental feature and may change in future releases.

