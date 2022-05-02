## ATTENTION: This is still WIP and shouldn't be used by anyone

# rpico-euro-seq

A Raspberry Pi Pico-based sequencer for Eurorack.

## Running on Pico

[schematic still missing, check source code for pins]

Don't forget to change Cargo.toml to point to a local rp2040-hal with the UART config code patched.

```sh
$ cargo install cargo-embed
$ DEFMT_LOG=info cargo embed --release
```


## Running Emulator

```sh
$ cargo install wasm-pack
$ cd emulator
$ npm i
$ npm start
```