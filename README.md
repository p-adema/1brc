# 1BRC Rust implementation

This is an implementation of the 1BRC parsing / aggregation challenge is Rust, using mainly the standard library (and
`memchr` to find newlines).

It runs a single reader thread, which fills up buffers that are read from by parser threads. The parsing process is
generally zero-copy: after data is read from the file by the reader thread, the data is only copied once per unique
station name (per thread, to fill up the HashMap). This is accomplished by using a wrapper around the standard HashMap
that allows for using the Entry API with a reference type. The buffer itself is implemented without any atomic locks:
since the access pattern is relatively simple, `Arc` and `Mutex` can be avoided.

On my machine (M2 Macbook Air), it runs in read time: it takes ~8s to read in the file without parsing, and the same
amount of time to also parse the file. With faster SSD's, your mileage may vary.
This implementation runs around twice as fast as the reference implementation (~20s) on my machine.