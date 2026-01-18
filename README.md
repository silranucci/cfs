# cfs
Just a porting in Rust of what the great [Liz Rice](https://www.youtube.com/watch?v=8fi7uSYlOdc) did in Go.
Used `libc` crate.

```bash
docker run --privileged -it -v $(pwd):/app -w /app rust:1.89 bash
```

then inside the container

```bash
cargo run -- run  /bin/bash
```
