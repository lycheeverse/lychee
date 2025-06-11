# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.19.0](https://github.com/lycheeverse/lychee/compare/lychee-v0.18.1...lychee-v0.19.0) - 2025-06-11

Most notably with this release the deprecated `--exclude-mail` flag was removed and the behavior of the `--accept` flag was updated. Previously, status codes such as `200 OK` were always accepted. Now they are only accepted by default. This means providing the argument `--accept 201` now rejects status code `200 OK`.

### Added

- Raise error when the default config file is invalid ([#1715](https://github.com/lycheeverse/lychee/pull/1715))
- Detect website fragments ([#1675](https://github.com/lycheeverse/lychee/pull/1675))
- Move archive functionality to library ([#1720](https://github.com/lycheeverse/lychee/pull/1720))
- Add TLS version option ([#1655](https://github.com/lycheeverse/lychee/pull/1655))
- Add FreeBSD-Ask to users ([#1662](https://github.com/lycheeverse/lychee/pull/1662))
- Add support for custom file extensions in link checking. ([#1559](https://github.com/lycheeverse/lychee/pull/1559))
- Add support for custom headers in input processing ([#1561](https://github.com/lycheeverse/lychee/pull/1561))
- Add possible values for minimum TLS version in help message ([#1693](https://github.com/lycheeverse/lychee/pull/1693))

### Fixed

- Only check fragments inside files ([#1713](https://github.com/lycheeverse/lychee/pull/1713))
- Update --accept behaviour (https://github.com/lycheeverse/lychee/issues/1661)
- Allow header values that contain equal signs ([#1647](https://github.com/lycheeverse/lychee/pull/1647))

### Other

- Remove deprecated `--exclude-mail` flag ([#1669](https://github.com/lycheeverse/lychee/issues/1669))
- Bump the dependencies group across 1 directory with 3 updates ([#1714](https://github.com/lycheeverse/lychee/pull/1714))
- Upgrade to 2024 edition ([#1711](https://github.com/lycheeverse/lychee/pull/1711))
- Fix `test_exclude_example_domains` ([#1712](https://github.com/lycheeverse/lychee/pull/1712))
- Fix lints ([#1705](https://github.com/lycheeverse/lychee/pull/1705))
- Bump the dependencies group with 2 updates
- Bump the dependencies group across 1 directory with 11 updates ([#1692](https://github.com/lycheeverse/lychee/pull/1692))
- Specify MSRV ([#1676](https://github.com/lycheeverse/lychee/pull/1676))
- Make clippy happy ([#1681](https://github.com/lycheeverse/lychee/pull/1681))
- Bump the dependencies group with 3 updates ([#1670](https://github.com/lycheeverse/lychee/pull/1670))
- Fix accept/exclude range syntax and docs ([#1668](https://github.com/lycheeverse/lychee/pull/1668))
- Bump the dependencies group with 4 updates ([#1664](https://github.com/lycheeverse/lychee/pull/1664))
- Bump the dependencies group with 11 updates ([#1656](https://github.com/lycheeverse/lychee/pull/1656))
- Bump the dependencies group across 1 directory with 14 updates ([#1653](https://github.com/lycheeverse/lychee/pull/1653))
- Format Markdown URLs with `<>` instead of `[]()` ([#1638](https://github.com/lycheeverse/lychee/pull/1638))
- Bump the dependencies group across 1 directory with 21 updates ([#1643](https://github.com/lycheeverse/lychee/pull/1643))
- Add tests for URL extraction ending with a period ([#1641](https://github.com/lycheeverse/lychee/pull/1641))
- Renamed `base` to `base_url` (fixes #1607) ([#1629](https://github.com/lycheeverse/lychee/pull/1629))

## [0.18.1](https://github.com/lycheeverse/lychee/compare/lychee-v0.18.0...lychee-v0.18.1) - 2025-02-06

### Fixed

- do not fail on empty # and #top fragments (#1609)

### Other

- Fix Porgressbar rendering Checkbox (Fixes [#1626](https://github.com/lycheeverse/lychee/pull/1626)) ([#1627](https://github.com/lycheeverse/lychee/pull/1627))
- Add Checkbox Formatting Option for Markdown Reports ([#1623](https://github.com/lycheeverse/lychee/pull/1623))
- Fix new clippy lints ([#1625](https://github.com/lycheeverse/lychee/pull/1625))
- Bump the dependencies group with 3 updates ([#1604](https://github.com/lycheeverse/lychee/pull/1604))
- Bump the dependencies group with 3 updates ([#1602](https://github.com/lycheeverse/lychee/pull/1602))
- Bump the dependencies group with 6 updates ([#1597](https://github.com/lycheeverse/lychee/pull/1597))

## [0.18.0](https://github.com/lycheeverse/lychee/compare/lychee-v0.17.0...lychee-v0.18.0) - 2024-12-18

### Other

- Bump the dependencies group across 1 directory with 11 updates ([#1589](https://github.com/lycheeverse/lychee/pull/1589))
- Introduce --root-dir ([#1576](https://github.com/lycheeverse/lychee/pull/1576))
- Fix retries ([#1573](https://github.com/lycheeverse/lychee/pull/1573))
- Pass along --max-retries config option ([#1572](https://github.com/lycheeverse/lychee/pull/1572))
- Bump the dependencies group with 4 updates ([#1571](https://github.com/lycheeverse/lychee/pull/1571))
- Bump the dependencies group with 4 updates ([#1570](https://github.com/lycheeverse/lychee/pull/1570))
- Bump the dependencies group with 4 updates ([#1566](https://github.com/lycheeverse/lychee/pull/1566))
- Rename `fail_map` to `error_map` for improved clarity in response statistics ([#1560](https://github.com/lycheeverse/lychee/pull/1560))
- Add quirks support for `youtube-nocookie.com` and youtube embed URLs ([#1563](https://github.com/lycheeverse/lychee/pull/1563))
- Support excluded paths in `--dump-inputs` ([#1556](https://github.com/lycheeverse/lychee/pull/1556))
- Improve robustness of cache integration test ([#1557](https://github.com/lycheeverse/lychee/pull/1557))
- Bump the dependencies group across 1 directory with 7 updates ([#1552](https://github.com/lycheeverse/lychee/pull/1552))
- Bring back error output for links (#1553)

## [0.17.0](https://github.com/lycheeverse/lychee/compare/lychee-v0.16.1...lychee-v0.17.0) - 2024-11-06

### Fixed

- Remove tokio console subscriber ([#1524](https://github.com/lycheeverse/lychee/pull/1524))

### Other

- Bump the dependencies group across 1 directory with 12 updates ([#1544](https://github.com/lycheeverse/lychee/pull/1544))
- Ignore casing when processing markdown fragments + check for percent encoded ancors ([#1535](https://github.com/lycheeverse/lychee/pull/1535))
- Refactor cache handling test to make it more robust ([#1548](https://github.com/lycheeverse/lychee/pull/1548))
- Fix format option in configuration file ([#1547](https://github.com/lycheeverse/lychee/pull/1547))
- Fix skipping of email addresses in stylesheets ([#1546](https://github.com/lycheeverse/lychee/pull/1546))
- Add support for relative links ([#1489](https://github.com/lycheeverse/lychee/pull/1489))
- Update `pkg-url` of cargo binstall ([#1532](https://github.com/lycheeverse/lychee/pull/1532))
- Bump the dependencies group with 3 updates ([#1530](https://github.com/lycheeverse/lychee/pull/1530))
- Allow excluding cache based on status code ([#1403](https://github.com/lycheeverse/lychee/pull/1403))
- Respect timeout when retrieving archived link ([#1526](https://github.com/lycheeverse/lychee/pull/1526))
- Disable Wayback machine tests
- Bump the dependencies group with 6 updates ([#1516](https://github.com/lycheeverse/lychee/pull/1516))
