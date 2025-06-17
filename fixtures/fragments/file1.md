# Fragment Test File 1

This is a test file for the fragment loader.

## Fragment 1

[Link to fragment 2](#fragment-2)

## Fragment 2

[Link to fragment 1 in file2](file2.md#fragment-1)

## Fragment 3

[Link to missing fragment](#missing-fragment)

[Link to missing fragment in file2](file2.md#missing-fragment)

### `Code` ``Heading
[Link to code heading](#code-heading)

## HTML Fragments

Explicit fragment links are currently not supported.
Therefore we put the test into a code block for now to prevent false positives.

<a id="explicit-fragment"></a>

[Link to explicit fragment](#explicit-fragment)

[To the HTML doc](file.html#a-word)

## Custom Fragments

[Custom fragment id in file2](file2.md#custom-id)

# Kebab Case Fragment

[Link to kebab-case fragment](#kebab-case-fragment)

[Link to second kebab-case fragment](#kebab-case-fragment-1)

# Kebab Case Fragment

[Link to another file type](empty_file#fragment)

# Ignore casing

[Link with wrong casing](#IGNORE-CASING)

# Fünf süße Äpfel

[Link to umlauts](#fünf-süße-äpfel)
[Link to umlauts wrong case](#fünf-sÜße-Äpfel)
[Link to umlauts with percent encoding](#f%C3%BCnf-s%C3%BC%C3%9Fe-%C3%A4pfel)

# To top fragments

The empty "#" and "#top" fragments are always valid
without related HTML element. Browser will scroll to the top of the page.

[Link to top of file2](file2.md#)
[Alternative link to top of file2](file2.md#top)

##### Lets wear a hat: être

A link to the non-existing fragment: [try](https://github.com/lycheeverse/lychee#non-existent-anchor).

Skip the fragment check for directories like: [empty](empty_dir/).

# Binary data URLs checks

Fragment checking tries to scan the (whole) content/response body for HTML element IDs.
This fails for binary data and can cause unnecessary traffic for remote URLs.

## Without fragment

Fragment checking is skipped if the URL does not actually contain a fragment.
Even with fragment checking enabled, the following links must hence succeed:

[Link to local binary file without fragment](zero.bin)
[Link to local binary file with empty fragment](zero.bin#)
[Link to remote binary file without fragment](https://raw.githubusercontent.com/MichaIng/lychee/skip-fragment-check-by-url/fixtures/fragments/zero.bin)
[Link to remote binary file with empty fragment](https://raw.githubusercontent.com/MichaIng/lychee/skip-fragment-check-by-url/fixtures/fragments/zero.bin#)

## Local file with fragment

For local files URIs with fragment, the fragment checker is invoked and fails to read the content,
but the file checker emits a warning only. The following link hence must succeed as well:

[Link to local binary file with fragment](zero.bin#fragment)

## Remote URL with fragment

Right now, there is not MIME/content type based exclusion for fragment checks in the website checker.
Also, other than the file checker, the website checker throws an error if reading the response body fails.
The following link hence must fail:

[Link to remote binary file with fragment](https://raw.githubusercontent.com/MichaIng/lychee/skip-fragment-check-by-url/fixtures/fragments/zero.bin#fragment)
