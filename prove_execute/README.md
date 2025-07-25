A small tool that allows you to call prove & execute.

Example use:

This will try to send a snark that should cover blocks from 2 - 4.
It is going to send it in ohbender verifier format.
```
cargo run -- --address 0xE43497368Cd61e38F7976ab065DEb61fEE374248 --start 2 --end 4 --snark-path merged_3.snark
```

If you don't want to spend time creating a snark, you can try to use the 'mock' verifier -- then you have to compute the proper public input, and pass it directly:


```
cargo run -- --address 0x19ed66b7e720aA741a748d1779da66D30FDCa549  --start 1 --end 1  --public-input fbc56cf7a8cf1666e1d6a1fb80f23f9efa07965eed1016ab
```