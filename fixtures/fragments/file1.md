# Fragment Test File 1

This is a test file for the fragment loader.

## Fragment 1

[Link to fragment 2](#fragment-2)

## Fragment 2

[Link to fragment 1 in file2](file2.md#fragment-1)

## Fragment 3

[Link to missing fragment](#missing-fragment)

[Link to missing fragment in file2](file2.md#missing-fragment)

## HTML Fragments

Explicit fragment links are currently not supported.
Therefore we put the test into a code block for now to prevent false positives.

<a id="explicit-fragment"></a>

[Link to explicit fragment](#explicit-fragment)

[To the html doc](file.html#a-word)

## Custom Fragments

[Custom fragment id in file2](file2.md#custom-id)

# Kebab Case Fragment

[Link to kebab-case fragment](#kebab-case-fragment)

[Link to second kebab-case fragment](#kebab-case-fragment-1)

# Kebab Case Fragment

[Link to another file type](empty_file#fragment)
