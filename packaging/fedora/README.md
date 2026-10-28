# Fedora / RHEL package

Built with [cargo-generate-rpm](https://github.com/cat-in-136/cargo-generate-rpm) from the
`[package.metadata.generate-rpm]` section of `Cargo.toml`:

```sh
cargo install --locked cargo-generate-rpm
cargo build --release && strip target/release/mnnn
cargo generate-rpm             # -> target/generate-rpm/muninn-<ver>-1.x86_64.rpm
rpm -qlp target/generate-rpm/muninn-*.rpm
```

Requires are detected from the binary (glibc, libgcc); SQLCipher and OpenSSL are
statically linked.
