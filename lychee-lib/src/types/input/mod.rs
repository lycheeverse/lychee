//! Input handling modules for lychee
//!
//! The modules in this directory work together to provide all the mechanisms
//! for input handling in lychee.
//!
//! At its core, lychee is one big state machine, which takes various kinds of
//! input sources, resolves them into concrete, processable sources, and
//! extracts their content for link checking.
//!
//! There are a few main components involved in this process:
//! - [`Input`]: The high-level interface that users interact with to provide
//!   input sources and retrieve processed content.
//! - [`InputSource`]: Represents the different kinds of input sources lychee
//!   can handle, such as URLs, file paths, glob patterns, standard input, and
//!   raw strings.
//! - [`InputContent`]: Encapsulates the actual content extracted from an input
//!   source, along with metadata about the source and file type.
//! - [`InputResolver`]: The main driver that orchestrates the entire input
//!   processing pipeline, resolving input sources and extracting their content.
//! - [`ResolvedInputSource`]: Represents input sources after glob expansion --
//!   no more glob patterns!

// Make an exception here to allow using the same name for the module and its
// sub-modules. We could name this `core`, but `input` is more intuitive.
#[allow(clippy::module_inception)]
pub mod input;

pub mod content;
pub mod resolver;
pub mod source;

pub use content::InputContent;
pub use input::Input;
pub use resolver::InputResolver;
pub use source::InputSource;
pub use source::ResolvedInputSource;
