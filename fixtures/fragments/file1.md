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
