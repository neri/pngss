# A subset implementation of the PNG decoder

## Features

* Pure Rust Implementation
* Support for `no_std`
* It generally provides sufficient functionality for most applications, but some features are not supported.

## Supported feature

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

### MSRV

* The latest version is recommended whenever possible.

## LICENSE

MIT License

(c) nerry
