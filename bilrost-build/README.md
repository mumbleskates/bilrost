# `bilrost-build`

`bilrost-build` makes it easy to generate Rust code from `.proto` files as part of
a Cargo build. See the crate [documentation](https://docs.rs/bilrost-build/) for examples
of how to integrate `bilrost-build` into a Cargo project.

## `protoc`

`bilrost-build` uses `protoc` to parse the proto files. There are two ways to make `protoc`
available for `bilrost-build`:

* Include `protoc` in your `PATH`. This can be done by following the [`protoc` install instructions].
* Pass the `PROTOC=<my/path/to/protoc>` environment variable with the path to
  `protoc`.

[`protoc` install instructions]: https://github.com/protocolbuffers/protobuf#protocol-compiler-installation

## License

`bilrost-build` is distributed under the terms of the Apache License (Version 2.0).

See [LICENSE](../LICENSE) for details.

Copyright 2017 Dan Burkert
