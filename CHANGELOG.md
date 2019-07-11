
# Changelog for Vector v0.4.0-dev

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## v0.4.0-dev

### Added

- aws_s3: Add `filename_extension` options.
- aws_cloudwatch_logs: `stream_name` now accepts `{{key}}` synatx for extracting values from events.

### Changed

- aws_cloudwatch_logs: Now partitions events by `log_group`/`log_stream`.

### Deprecated

### Fixed

- aws_s3: Fixed #517 and trailing slash issues with the generated key.
- aws_cloudwatch_logs: Fixes #586 and now dynamically creates streams if they do not exist.

### Removed

### Security

## v0.3.X

The CHANGELOG for v0.3.X releases can be found in the [v0.3 branch](https://github.com/timberio/vector/blob/v0.3/CHANGELOG.md).

