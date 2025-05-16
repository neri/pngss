# A subset implementation of the PNG decoder

## Features

* Pure Rust Implementation
* Support for `no_std`
* It generally provides sufficient functionality for most applications, but some features are not supported.

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

## References

* https://www.w3.org/TR/png/

## LICENSE

MIT License

(c) nerry
