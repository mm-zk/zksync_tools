A small tool that allows you to call prove & execute.

Example use:

This will try to send a snark that should cover blocks from 2 - 4.
It is going to send it in ohbender verifier format.
```
cargo run -- --address 0xE43497368Cd61e38F7976ab065DEb61fEE374248 --start 2 --end 4 --snark-path merged_3.snark
```

