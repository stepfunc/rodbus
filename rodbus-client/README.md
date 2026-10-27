
Rodbus-client is a command line application that uses the [Rodbus](https://crates.io/crates/rodbus) crate
to send Modbus requests and print responses to the console.

Run it from a checkout of this repository:

```
> cargo run -p rodbus-client -- <ARGS>
```

or install it from the checkout with `cargo install --path rodbus-client`.

The first argument selects the transport, `tcp` or `serial`, and the request comes after it. Options
that apply to every transport go before the transport:

- `-i`: the Modbus unit ID (default 1)
- `-p`: send the request periodically, every this many milliseconds

The `tcp` transport takes:

- `--host`: the address of the server (default `127.0.0.1:502`)

The `serial` transport takes:

- `-p`: the path of the serial port (required)
- `-b`: baud rate (default 9600)
- `-d`: data bits, 5 to 8 (default 8)
- `-s`: stop bits, 1 or 2 (default 1)
- `--parity`: `none`, `odd` or `even` (default `none`)
- `-f`: flow control, `none`, `software` or `hardware` (default `none`)

Each request is a subcommand of the transport:

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
    - `-v`: write ON; leave it out to write OFF
- `wsr`: write single register
    - `-i`: index of the register
    - `-v`: value of the register
- `wmc`: write multiple coils
    - `-s`: starting address
    - `-v`: values of the coils (e.g. 10100011)
- `wmr`: write multiple registers
    - `-s`: starting address
    - `-v`: values of the registers as a comma delimited list (e.g. 1,4,7)

`--help` lists the options at each level, e.g. `cargo run -p rodbus-client -- tcp rc --help`.

Examples:

- Read coils 10 to 19 on `127.0.0.1`, port 502, unit ID 2: `cargo run -p rodbus-client -- -i 2 tcp
  --host 127.0.0.1:502 rc -s 10 -q 10`
- Read holding registers 10 to 19: `cargo run -p rodbus-client -- tcp rhr -s 10 -q 10`
- Write ON to coil 10: `cargo run -p rodbus-client -- tcp wsc -i 10 -v`
- Write multiple coils: `cargo run -p rodbus-client -- tcp wmc -s 10 -v 101001`
- Write register 10: `cargo run -p rodbus-client -- tcp wsr -i 10 -v 76`
- Write 42 to registers 10, 11 and 12: `cargo run -p rodbus-client -- tcp wmr -s 10 -v 42,42,42`
- Read input registers 0 to 4 over a serial port at 19200 baud: `cargo run -p rodbus-client --
  serial -p /dev/ttyUSB0 -b 19200 rir -s 0 -q 5`

To send a read coils request every 2 seconds:
`cargo run -p rodbus-client -- -p 2000 tcp rc -s 10 -q 10`
