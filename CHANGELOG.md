# Changelog

New features, improvements, and fixes in Zapbeam.

## Unreleased

### Added

- Automatic lossless JPEG and PNG optimization with smaller validated results replacing originals.
- Processing states, current file sizes, actual savings, and per-file error details.
- JPEG and PNG file selection through the file picker, drag and drop, or Cmd/Ctrl+O.
- A compact file list with current sizes and duplicate detection.
- Multi-selection and list removal with standard desktop shortcuts.
- Feedback for unsupported or inaccessible files.

### Changed

- JPEG optimization uses progressive scan optimization for greater lossless savings.
- Smaller optimized files omit private camera, GPS, date and comment metadata while retaining orientation and color information.

### Fixed

- File access metadata updates no longer falsely report that image contents changed during optimization.
- macOS screenshots now optimize successfully when recompression removes their obsolete data-offset index.
- macOS-managed filesystem attributes no longer prevent replacement of otherwise valid smaller images.
- Photos up to 64 megapixels can be validated with lower memory overhead.
- Specific error details explain when Content Credentials or HDR/multi-picture data prevents safe optimization.
- macOS completion reporting no longer waits for access to the image’s parent folder after replacement.
