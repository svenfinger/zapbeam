# Zapbeam

Zapbeam is an open-source project building a desktop media optimizer for macOS and Linux, inspired by ImageOptim.

Its focus is simple: a minimal interface, responsive interactions, and efficient compression that balances quality and speed. Local-first operation keeps processing on your machine.

Drop JPEG or PNG images into Zapbeam, or choose **Add Files**, to start lossless optimization automatically. Smaller validated results replace the originals; the table shows the current size and actual savings. Processing stays on your machine. Removing a row only removes it from the list.

Smaller replacements preserve orientation, color information and transparency while removing private camera details, GPS locations, dates and comments.

## Contributing

See the [contribution guide](CONTRIBUTING.md) for setup, development, validation, and build instructions. Changes are recorded in the [changelog](CHANGELOG.md). See the [release build guide](RELEASE.md) for packaging commands and supported targets.

## License

Zapbeam’s original code is licensed under the [MIT License](LICENSE). Third-party dependencies retain their own licenses; see the [codec notices](docs/licenses/MOZJPEG.md) and [Rust dependency notices](docs/licenses/RUST.txt). This software is based in part on the work of the Independent JPEG Group.
