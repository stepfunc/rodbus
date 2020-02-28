
Rodbus-client is a command line application that uses the [Rodbus](https://crates.io/crates/rodbus) crate
to send Modbus requests and print responses to the console. 
 
```
> cargo install rodbus-client
```

Use the `-h` option to specify the host to connect to and the `-i` option to
specify the Modbus unit ID.

Each request can be sent using the following subcommands:

- `rc`: read coils
    - `-s`: starting address
    - `-q`: quantity of coils
- `rdi`: read discrete inputs
    - `-s`: starting address
    - `-q`: quantity of discrete inputs
- `rhr`: read holding registers
    - `-s`: starting address
    - `-q`: quantity of holding registers
- `rir`: read input registers
    - `-s`: starting address
    - `-q`: quantity of input registers
- `wsc`: write single coil
    - `-i`: index of the coil
    - `-v`: value of the coil (`true` or `false`)
- `wsr`: write single register
    - `-i`: index of the register
    - `-v`: value of the register
- `wmc`: write multiple coils
    - `-s`: starting address
    - `-v`: values of the coils (e.g. 10100011)
- `wmr`: write multiple registers
    - `-s`: starting address
    - `-v`: values of the registers as a comma delimited list (e.g. 1,4,7)

Examples:

- Read coils 10 to 19 on `localhost`, port 502, unit ID `0x02`: `cargo run -p rodbus-client -- -h
  127.0.0.1:502 -i 2 rc -s 10 -q 10`
- Read holding registers 10 to 19: `cargo run -p rodbus-client -- rhr -s 10 -q 10`
- Write coil 10: `cargo run -p rodbus-client -- wsc -i 10 -v true`
- Write multiple coils: `cargo run -p rodbus-client -- wmc -s 10 -v 101001`
- Write register 10: `cargo run -p rodbus-client -- wsr -i 10 -v 76`
- Write 42 to registers 10, 11 and 12: `cargo run -p rodbus-client -- wmr -s 10
  -v 42,42,42`

It is also possible to send periodic requests with the `-p` argument. For example,
to send a read coils request every 2 seconds, you would do this:
`cargo run -p rodbus-client -- -p 2000 rc -s 10 -q 10`

## License

Licensed under the 3-Clause BSD License. See [LICENSE.md](./LICENSE.md) for more
details.

Copyright 2019-2020 Automatak LLC. All rights reserved.
