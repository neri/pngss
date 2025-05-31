# pngss: A subset implementation of PNG Encoder and Decoder

## Features

* Pure Rust Implementation
* Support for `no_std`
* It generally provides sufficient functionality for most applications, but some features are not supported.
* The default deflate library uses our implementation. We are constantly improving it, but if you are not satisfied with its performance, you can implement a custom class to replace it with another implementation. See also `DeflateDecoder` or `DeflateEncoder`.
* The detailed specifications are subject to change as it is still under development.

### MSRV

* Undetermined, The latest version is recommended whenever possible.

### Supported features

|feature|supported|
|-|-|
|IHDR chunk|✅|
|PLTE chunk|✅|
|IDAT chunk|✅|
|IEND chunk|✅|
|8bit depth color|✅|
|16bit depth color|-|
|Interlace|-|
|Color space|-|
|CRC check|-|

## Example Apps

### /viewer: Image Viewer

* An example application to display PNG files using `embedded-graphics`

```sh
$ cargo run -p viewer FILE_NAME
```

## /idatdump: IDAT dumper

* An example application showing the contents of IDAT chunks

```sh
$ cargo run -p idatdump -- [MODE] [OPTIONS] [--] FILE_NAME
```

### Modes:
* `-raw`       Decompress IDAT chunks and dump them as is **(default)**
* `-filter`    Decompress IDAT chunks to show only filter types
* `-decoded`   Dumps the final decoding results of the image data

### Options:

* `-hex`    Show hex dump **(default)**
* `-bin`    Show binary
* `-b64`    Show in base64

## References

* https://www.w3.org/TR/png/

## LICENSE

MIT License

(c) nerry
