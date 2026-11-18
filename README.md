# HexGame

A hex map strategy game foundation in Rust: a port of the C++
[hexGame](https://github.com/sstanfield/hexGame) engine demo. The UI is
[Turbo Ferret](https://github.com/sstanfield/turbo-ferret) and graphics go through
[miniquad](https://github.com/not-fl3/miniquad).

## Build and run

```sh
cargo run
```

Rust 1.86 or newer. On Linux, miniquad needs
`libx11-dev libxi-dev libgl1-mesa-dev libasound2-dev`.

## Licence

zlib (see `LICENSE`). `NOTICE` lists what the game is built on and ships with.
