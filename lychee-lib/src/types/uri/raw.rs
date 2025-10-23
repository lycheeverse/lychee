use std::{fmt::Display, num::NonZeroUsize};

/// A raw URI that got extracted from a document with a fuzzy parser.
/// Note that this can still be invalid according to stricter URI standards
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RawUri {
    /// Unparsed URI represented as a `String`. There is no guarantee that it
    /// can be parsed into a URI object
    pub text: String,
    /// Name of the element that contained the URI (e.g. `a` for the <a> tag).
    /// This is a way to classify links to make it easier to offer fine control
    /// over the links that will be checked e.g. by trying to filter out links
    /// that were found in unwanted tags like `<pre>` or `<code>`.
    pub element: Option<String>,
    /// Name of the attribute that contained the URI (e.g. `src`). This is a way
    /// to classify links to make it easier to offer fine control over the links
    /// that will be checked e.g. by trying to filter out links that were found
    /// in unwanted attributes like `srcset` or `manifest`.
    pub attribute: Option<String>,
    /// The position of the URI in the document.
    pub span: RawUriSpan,
}

impl Display for RawUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} (Attribute: {:?})", self.text, self.attribute)
    }
}

#[cfg(test)]
impl From<(&str, RawUriSpan)> for RawUri {
    fn from((text, span): (&str, RawUriSpan)) -> Self {
        RawUri {
            text: text.to_string(),
            element: None,
            attribute: None,
            span,
        }
    }
}

/// A span of a [`RawUri`] in the document.
///
/// The span can be used to give more precise error messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RawUriSpan {
    /// The line of the URI.
    ///
    /// The line is 1-based.
    pub line: NonZeroUsize,
    /// The column of the URI if computable.
    ///
    /// The column is 1-based.
    /// This is `None`, if the column can't be computed exactly,
    /// e.g. when it comes from the `html5ever` parser.
    pub column: Option<NonZeroUsize>,
}

/// Test helper to create [`RawUriSpan`]s easily.
#[cfg(test)]
pub(crate) const fn span(line: usize, column: usize) -> RawUriSpan {
    RawUriSpan {
        line: NonZeroUsize::new(line).unwrap(),
        column: Some(NonZeroUsize::new(column).unwrap()),
    }
}

/// Test helper to create a [`RawUriSpan`] from just the line and leave the column unset.
#[cfg(test)]
pub(crate) const fn span_line(line: usize) -> RawUriSpan {
    RawUriSpan {
        line: std::num::NonZeroUsize::new(line).unwrap(),
        column: None,
    }
}

/// A trait for calculating a [`RawUriSpan`] at a given byte offset in the document.
///
/// If you have a document and want spans with absolute positions, use [`SourceSpanProvider`].
/// If you start inside a document at a given offset, use [`OffsetSpanProvider`].
pub(crate) trait SpanProvider {
    /// Compute the [`RawUriSpan`] at a given byte offset in the document.
    fn span(&self, offset: usize) -> RawUriSpan;
}

/// A [`SpanProvider`] which calculates spans depending on the input lines.
///
/// Precomputes line lengths so that constructing [`RawUriSpan`]s is faster.
/// If you start inside a document at a given offset, consider using [`OffsetSpanProvider`].
#[derive(Clone, Debug)]
pub(crate) struct SourceSpanProvider<'a> {
    /// The computed map from line number to offset in the document.
    line_starts: Vec<usize>,
    /// The input document.
    ///
    /// This is used to compute column information, since we can't rely on each character being a
    /// single byte long.
    input: &'a str,
}

impl<'a> SourceSpanProvider<'a> {
    /// Create a [`SpanProvider`] from the given document.
    ///
    /// If the input is part of a larger document, consider using [`OffsetSpanProvider`] instead.
    ///
    /// This function isn't just a simple constructor but does some work, so call this only if you
    /// want to use it.
    pub(crate) fn from_input(input: &'a str) -> Self {
        // FIXME: Consider making this lazy?
        let line_starts: Vec<_> = core::iter::once(0)
            .chain(input.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Self { line_starts, input }
    }
}

impl SpanProvider for SourceSpanProvider<'_> {
    fn span(&self, offset: usize) -> RawUriSpan {
        const ONE: NonZeroUsize = NonZeroUsize::MIN;
        let line = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        // Since we get the index by the binary_search above and subtract `1` if it would be larger
        // than the length of the document, this shouldn't panic.
        let line_offset = self.line_starts[line];
        let column = self
            .input
            .get(line_offset..offset)
            .or_else(|| self.input.get(line_offset..))
            // columns are 1-based
            .map(|v| ONE.saturating_add(v.chars().count()));

        RawUriSpan {
            // lines are 1-based
            line: ONE.saturating_add(line),
            column,
        }
    }
}

/// A [`SpanProvider`] which starts at a given offset in the document.
///
/// All given offsets are changed by the given amount before computing the
/// resulting [`RawUriSpan`] with the inner [`SpanProvider`].
#[derive(Clone, Debug)]
pub(crate) struct OffsetSpanProvider<'a, T: SpanProvider = SourceSpanProvider<'a>> {
    /// The byte offset in the document by which all given offsets are changed before computing the
    /// resulting [`RawUriSpan`] with the inner [`SpanProvider`].
    pub(crate) offset: usize,
    /// The inner [`SpanProvider`] which will be used to determine the spans.
    pub(crate) inner: &'a T,
}

impl<T: SpanProvider> SpanProvider for OffsetSpanProvider<'_, T> {
    fn span(&self, offset: usize) -> RawUriSpan {
        self.inner.span(self.offset + offset)
    }
}
