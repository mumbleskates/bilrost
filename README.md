# ICE in trait resolution

To reproduce, build the crate in `./test-ice`.

Currently this is an ICE on `rustc 1.100.0-nightly (0ed41eb41 2026-09-04)`
(with the new trait solver) and causes `rustc 1.98.1` to consume resources
indefinitely without visible progress.
