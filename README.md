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
|16bit color|-|
|Interlace|-|
|Color space|-|
|CRC check|-|

## LICENSE

MIT License

(c) nerry
