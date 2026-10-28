# Debian / Ubuntu package

Built with [cargo-deb](https://github.com/kornelski/cargo-deb) from the
`[package.metadata.deb]` section of `Cargo.toml`:

```sh
cargo install --locked cargo-deb
cargo deb                      # -> target/debian/muninn_<ver>-1_amd64.deb
dpkg-deb -c target/debian/muninn_*.deb && lintian target/debian/muninn_*.deb
```

Dependencies are set explicitly (`libc6`, `libgcc-s1`; SQLCipher and OpenSSL are
statically linked), so `dpkg-shlibdeps` is not needed to build.
