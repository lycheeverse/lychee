# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.20.1](https://github.com/lycheeverse/lychee/compare/lychee-lib-v0.20.0...lychee-lib-v0.20.1) - 2025-08-25

### Other

- Always check files explicitly if specified by user or matched by user glob ([#1816](https://github.com/lycheeverse/lychee/pull/1816))
- *(docs)* update TOC
- Bump the dependencies group across 1 directory with 3 updates ([#1812](https://github.com/lycheeverse/lychee/pull/1812))
- Update changelog

## [0.20.0](https://github.com/lycheeverse/lychee/compare/lychee-lib-v0.19.1...lychee-lib-v0.20.0) - 2025-08-21

### Added

- make wikilink extraction and checking opt-in ([#1803](https://github.com/lycheeverse/lychee/pull/1803))
- skip fragment checking for unsupported MIME types ([#1744](https://github.com/lycheeverse/lychee/pull/1744))
- add 'user-content-' prefix to support github markdown fragment ([#1750](https://github.com/lycheeverse/lychee/pull/1750))

### Fixed

- fix comment for `ErrorKind::InvalidFragment` ([#1775](https://github.com/lycheeverse/lychee/pull/1775))
- do not check the fragment when http response err but accepted ([#1763](https://github.com/lycheeverse/lychee/pull/1763))
- treat a fragment in an empty directory as an error ([#1756](https://github.com/lycheeverse/lychee/pull/1756))
- resolve index file inside a directory ([#1752](https://github.com/lycheeverse/lychee/pull/1752))
- skip fragment check if website URL doesn't contain fragment ([#1733](https://github.com/lycheeverse/lychee/pull/1733))

### Other

- Bump dependencies ([#1811](https://github.com/lycheeverse/lychee/pull/1811))
- Skip binary and invalid UTF-8 inputs ([#1810](https://github.com/lycheeverse/lychee/pull/1810))
- Refactor input dumping and path retrieval with extension filtering ([#1648](https://github.com/lycheeverse/lychee/pull/1648))
- Use a HashSet to store inputs and avoid duplicates ([#1781](https://github.com/lycheeverse/lychee/pull/1781))
- add `--index-files` flag, and turn off index file checking by default ([#1777](https://github.com/lycheeverse/lychee/pull/1777))
- Cleanup input example ([#1792](https://github.com/lycheeverse/lychee/pull/1792))
- FIx missing identifier in snap build action; add snap install to readme ([#1793](https://github.com/lycheeverse/lychee/pull/1793))
- Fix clippy lints; refactor code slightly
- refactor `test_fragments` to clarify expected successes/fails ([#1776](https://github.com/lycheeverse/lychee/pull/1776))
- Regular expressions for exclude_path ([#1766](https://github.com/lycheeverse/lychee/pull/1766))
- Fix basic auth ([#1748](https://github.com/lycheeverse/lychee/pull/1748))
- Update 'Users' section in the README
- Add ProseKit to users
- Migrate to Clippy 1.88 ([#1749](https://github.com/lycheeverse/lychee/pull/1749))
- Add xml schema found in xsd files to list of exclusions ([#1735](https://github.com/lycheeverse/lychee/pull/1735))

## [0.19.1](https://github.com/lycheeverse/lychee/compare/lychee-lib-v0.19.0...lychee-lib-v0.19.1) - 2025-06-16

### Fixed

- skip the fragment check if the uri doesn't contain fragment ([#1730](https://github.com/lycheeverse/lychee/pull/1730))

### Other

- Update changelog

## [0.19.0](https://github.com/lycheeverse/lychee/compare/lychee-lib-v0.18.1...lychee-lib-v0.19.0) - 2025-06-11

### Added

- Respect the `disabled` property for stylesheet links ([#1716](https://github.com/lycheeverse/lychee/pull/1716))
- Detect website fragments ([#1675](https://github.com/lycheeverse/lychee/pull/1675))

### Fixed

- Only check the fragment when it's a file ([#1713](https://github.com/lycheeverse/lychee/pull/1713))
- Ignore gitlab table of content in wikilinks ([#1710](https://github.com/lycheeverse/lychee/pull/1710))

### Other

- Update --accept behaviour [#1661](https://github.com/lycheeverse/lychee/issues/1661)
- Move archive functionality to library ([#1720](https://github.com/lycheeverse/lychee/pull/1720))
- Bump the dependencies group across 1 directory with 3 updates ([#1714](https://github.com/lycheeverse/lychee/pull/1714))
- Upgrade to 2024 edition ([#1711](https://github.com/lycheeverse/lychee/pull/1711))
- Add support for custom headers in input processing ([#1561](https://github.com/lycheeverse/lychee/pull/1561))
- Fix lints ([#1705](https://github.com/lycheeverse/lychee/pull/1705))
- Remove deprecated `--exclude-mail` flag ([#1669](https://github.com/lycheeverse/lychee/issues/1669))
- Detect wikilinks, prevent plaintext extraction from links #1650 ([#1679](https://github.com/lycheeverse/lychee/pull/1679))
- Bump the dependencies group with 2 updates
- Add possible values for minimum TLS version in help message ([#1693](https://github.com/lycheeverse/lychee/pull/1693))
- Add TLS version option ([#1655](https://github.com/lycheeverse/lychee/pull/1655))
- Bump the dependencies group across 1 directory with 11 updates ([#1692](https://github.com/lycheeverse/lychee/pull/1692))
- Specify MSRV ([#1676](https://github.com/lycheeverse/lychee/pull/1676))
- Fix outdated link
- Remove once_cell as direct dependency
- Make clippy happy ([#1681](https://github.com/lycheeverse/lychee/pull/1681))
- Bump the dependencies group with 3 updates ([#1670](https://github.com/lycheeverse/lychee/pull/1670))
- Fix accept/exclude range syntax and docs ([#1668](https://github.com/lycheeverse/lychee/pull/1668))
- Add FreeBSD-Ask to users ([#1662](https://github.com/lycheeverse/lychee/pull/1662))
- Bump the dependencies group with 4 updates ([#1664](https://github.com/lycheeverse/lychee/pull/1664))
- Bump the dependencies group with 11 updates ([#1656](https://github.com/lycheeverse/lychee/pull/1656))
- Bump the dependencies group across 1 directory with 14 updates ([#1653](https://github.com/lycheeverse/lychee/pull/1653))
- Add support for custom file extensions in link checking. ([#1559](https://github.com/lycheeverse/lychee/pull/1559))
- Bump the dependencies group across 1 directory with 21 updates ([#1643](https://github.com/lycheeverse/lychee/pull/1643))
- Renamed `base` to `base_url` (fixes #1607) ([#1629](https://github.com/lycheeverse/lychee/pull/1629))

## [0.18.1](https://github.com/lycheeverse/lychee/compare/lychee-lib-v0.18.0...lychee-lib-v0.18.1) - 2025-02-06

### Fixed

- do not fail on empty # and #top fragments (#1609)

### Other

- Fix new clippy lints ([#1625](https://github.com/lycheeverse/lychee/pull/1625))
- Bump the dependencies group with 3 updates ([#1604](https://github.com/lycheeverse/lychee/pull/1604))
- Bump the dependencies group with 3 updates ([#1602](https://github.com/lycheeverse/lychee/pull/1602))
- Bump the dependencies group with 6 updates ([#1597](https://github.com/lycheeverse/lychee/pull/1597))

## [0.18.0](https://github.com/lycheeverse/lychee/compare/lychee-lib-v0.17.0...lychee-lib-v0.18.0) - 2024-12-18

### Other

- Bump the dependencies group across 1 directory with 11 updates ([#1589](https://github.com/lycheeverse/lychee/pull/1589))
- Introduce --root-dir ([#1576](https://github.com/lycheeverse/lychee/pull/1576))
- Fix retries ([#1573](https://github.com/lycheeverse/lychee/pull/1573))
- Bump the dependencies group with 4 updates ([#1571](https://github.com/lycheeverse/lychee/pull/1571))
- Bump the dependencies group with 4 updates ([#1570](https://github.com/lycheeverse/lychee/pull/1570))
- Bump the dependencies group with 4 updates ([#1566](https://github.com/lycheeverse/lychee/pull/1566))
- Rename `fail_map` to `error_map` for improved clarity in response statistics ([#1560](https://github.com/lycheeverse/lychee/pull/1560))
- Add quirks support for `youtube-nocookie.com` and youtube embed URLs ([#1563](https://github.com/lycheeverse/lychee/pull/1563))
- Support underscores in Markdown URLs ([#1555](https://github.com/lycheeverse/lychee/pull/1555))
- Bump the dependencies group across 1 directory with 7 updates ([#1552](https://github.com/lycheeverse/lychee/pull/1552))
- Bring back error output for links (#1553)

## [0.17.0](https://github.com/lycheeverse/lychee/compare/lychee-lib-v0.16.1...lychee-lib-v0.17.0) - 2024-11-06

### Added

- Add tests for `dns-prefetch` ([#1522](https://github.com/lycheeverse/lychee/pull/1522))

### Other

- Bump the dependencies group across 1 directory with 12 updates ([#1544](https://github.com/lycheeverse/lychee/pull/1544))
- Ignore casing when processing markdown fragments + check for percent encoded ancors ([#1535](https://github.com/lycheeverse/lychee/pull/1535))
- Fix skipping of email addresses in stylesheets ([#1546](https://github.com/lycheeverse/lychee/pull/1546))
- Add support for relative links ([#1489](https://github.com/lycheeverse/lychee/pull/1489))
- Box Octocrab error as it is too large ([#1543](https://github.com/lycheeverse/lychee/pull/1543))
- Don't check prefix attribute ([#1536](https://github.com/lycheeverse/lychee/pull/1536))
- Bump the dependencies group with 3 updates ([#1530](https://github.com/lycheeverse/lychee/pull/1530))
- Allow excluding cache based on status code ([#1403](https://github.com/lycheeverse/lychee/pull/1403))
- Ignore textContent links in html nodes ([#1528](https://github.com/lycheeverse/lychee/pull/1528))
- Exclude `rel=dns-prefetch` links ([#1520](https://github.com/lycheeverse/lychee/pull/1520))
- Improve docs for fragment checker
- Don't check preconnect links ([#1187](https://github.com/lycheeverse/lychee/pull/1187))
- Bump the dependencies group with 6 updates ([#1516](https://github.com/lycheeverse/lychee/pull/1516))
