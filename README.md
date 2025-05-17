# A subset implementation of the PNG decoder

## Features

* Pure Rust Implementation
* Support for `no_std`
* It generally provides sufficient functionality for most applications, but some features are not supported.
* The detailed specifications are subject to change as it is still under development.

### MSRV

* The latest version is recommended whenever possible.

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

* Example application to display PNG files using `embedded-graphics`

```sh
$ cargo run -p viewer FILE_NAME
```

## References

* https://www.w3.org/TR/png/

## LICENSE

MIT License

(c) nerry
