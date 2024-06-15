// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Pretty-printing diagnostic messages containing references to source code
//!
//! This module defines some data types for constructing intermediate data
//! structures for printing diagnostic messages referencing source code
//! fragments.  When you have an [`Error`](crate::parser::Error), you can
//! convert it to a [`Message`]. Then, you can in turn convert it into
//! `annotate_snippets::Snippet`, for example, and finally format a printable
//! diagnostic message string.
//!
//! When the `yash_syntax` crate is built with the `annotate-snippets` feature
//! enabled, it supports conversion from `Message` to `Snippet`. If you would
//! like to use another formatter instead, you can provide your own conversion
//! for yourself.
//!
//! ## Printing an error
//!
//! This example shows how to format an [`Error`](crate::parser::Error) instance
//! into a human-readable string.
//!
//! ```
//! # use yash_syntax::parser::{Error, ErrorCause, SyntaxError};
//! # use yash_syntax::source::Location;
//! # use yash_syntax::source::pretty::Message;
//! let error = Error {
//!     cause: ErrorCause::Syntax(SyntaxError::EmptyParam),
//!     location: Location::dummy(""),
//! };
//! let message = Message::from(&error);
//! // The lines below require the `annotate-snippets` feature.
//! # #[cfg(feature = "annotate-snippets")]
//! # {
//! let message = annotate_snippets::Message::from(&message);
//! eprint!("{}", annotate_snippets::Renderer::plain().render(message));
//! # }
//! ```
//!
//! You can also implement conversion from your custom error object to a
//! [`Message`], which then can be used in the same way to format a diagnostic
//! message. To do this, you can either directly implement `From<YourError>` for
//! `Message`, or implement [`MessageBase`] for `YourError` thereby deriving
//! `From<&YourError>` for `Message`.

use super::Location;
use std::borrow::Cow;
use std::cell::Ref;
use std::ops::Deref;
use std::rc::Rc;

/// Type of annotation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnnotationType {
    Error,
    Warning,
    Info,
    Note,
    Help,
}

/// Source code fragment annotated with a label
///
/// Annotations are part of an entire [`Message`].
#[derive(Clone)]
pub struct Annotation<'a> {
    /// Type of annotation
    pub r#type: AnnotationType,
    /// String that describes the annotated part of the source code
    pub label: Cow<'a, str>,
    /// Position of the annotated fragment in the source code
    pub location: &'a Location,
    /// Annotated code string
    ///
    /// This value provides an access to the string held in
    /// `self.location.code.value`, which can only be accessed by a `Ref`.
    pub code: Rc<dyn Deref<Target = str> + 'a>,
}

impl std::fmt::Debug for Annotation<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Annotation")
            .field("type", &self.r#type)
            .field("label", &self.label)
            .field("location", &self.location)
            .field("code", &&**self.code)
            .finish()
    }
}

impl<'a> Annotation<'a> {
    /// Creates a new annotation.
    ///
    /// This function makes a borrow of `location.code.value` and stores it in
    /// `self.code`. If it has been mutually borrowed, this function panics.
    pub fn new(r#type: AnnotationType, label: Cow<'a, str>, location: &'a Location) -> Self {
        Annotation {
            r#type,
            label,
            location,
            code: Rc::new(Ref::map(location.code.value.borrow(), String::as_str)),
        }
    }
}

/// Additional text without associated source code
#[derive(Clone, Debug)]
pub struct Footer<'a> {
    /// Type of this footer
    pub r#type: AnnotationType,
    /// Text of this footer
    pub label: Cow<'a, str>,
}

/// Entire diagnostic message
#[derive(Clone, Debug)]
pub struct Message<'a> {
    /// Type of this message
    pub r#type: AnnotationType,
    /// String that communicates the most important information in this message
    pub title: Cow<'a, str>,
    /// References to source code fragments annotated with additional information
    pub annotations: Vec<Annotation<'a>>,
    /// Additional text without associated source code
    pub footers: Vec<Footer<'a>>,
}

impl super::Source {
    /// Appends complementary annotations describing this source.
    pub fn complement_annotations<'a, 's: 'a, T: Extend<Annotation<'a>>>(&'s self, result: &mut T) {
        use super::Source::*;
        match self {
            Unknown | Stdin | CommandString | CommandFile { .. } => (),
            CommandSubst { original } => {
                // TODO Use Extend::extend_one
                result.extend(std::iter::once(Annotation::new(
                    AnnotationType::Info,
                    "command substitution appeared here".into(),
                    original,
                )));
            }
            Arith { original } => {
                // TODO Use Extend::extend_one
                result.extend(std::iter::once(Annotation::new(
                    AnnotationType::Info,
                    "arithmetic expansion appeared here".into(),
                    original,
                )));
            }
            Eval { original } => {
                // TODO Use Extend::extend_one
                result.extend(std::iter::once(Annotation::new(
                    AnnotationType::Info,
                    "command passed to the eval built-in here".into(),
                    original,
                )));
            }
            DotScript { name, origin } => {
                // TODO Use Extend::extend_one
                result.extend(std::iter::once(Annotation::new(
                    AnnotationType::Info,
                    format!("script `{name}` was sourced here",).into(),
                    origin,
                )));
            }
            Trap { origin, .. } => {
                // TODO Use Extend::extend_one
                result.extend(std::iter::once(Annotation::new(
                    AnnotationType::Info,
                    "trap was set here".into(),
                    origin,
                )));
            }
            Alias { original, alias } => {
                // TODO Use Extend::extend_one
                result.extend(std::iter::once(Annotation::new(
                    AnnotationType::Info,
                    format!("alias `{}` was substituted here", alias.name).into(),
                    original,
                )));
                original.code.source.complement_annotations(result);
                result.extend(std::iter::once(Annotation::new(
                    AnnotationType::Info,
                    format!("alias `{}` was defined here", alias.name).into(),
                    &alias.origin,
                )));
                alias.origin.code.source.complement_annotations(result);
            }
        }
    }
}

/// Helper for constructing a [`Message`]
///
/// Thanks to the blanket implementation `impl<'a, T: MessageBase> From<&'a T>
/// for Message<'a>`, implementors of this trait can be converted to a message
/// for free.
pub trait MessageBase {
    /// Returns the type of the entire message.
    ///
    /// The default implementation returns `AnnotationType::Error`.
    fn message_type(&self) -> AnnotationType {
        AnnotationType::Error
    }

    // TODO message tag

    /// Returns the main caption of the message.
    fn message_title(&self) -> Cow<str>;

    /// Returns an annotation to be the first in the message.
    fn main_annotation(&self) -> Annotation<'_>;

    /// Adds additional annotations to the given container.
    ///
    /// The default implementation does nothing.
    fn additional_annotations<'a, T: Extend<Annotation<'a>>>(&'a self, results: &mut T) {
        let _ = results;
    }

    /// Returns footers that are included in the message.
    fn footers(&self) -> Vec<Footer> {
        Vec::new()
    }
}

/// Constructs a message based on the message base.
impl<'a, T: MessageBase> From<&'a T> for Message<'a> {
    fn from(base: &'a T) -> Self {
        let main_annotation = base.main_annotation();
        let main_source = &main_annotation.location.code.source;
        let mut annotations = vec![main_annotation];

        main_source.complement_annotations(&mut annotations);
        base.additional_annotations(&mut annotations);

        Message {
            r#type: base.message_type(),
            title: base.message_title(),
            annotations,
            footers: base.footers(),
        }
    }
}

#[cfg(feature = "annotate-snippets")]
mod annotate_snippets_support {
    use super::*;

    /// Converts `yash_syntax::source::pretty::AnnotationType` into
    /// `annotate_snippets::Level`.
    ///
    /// This implementation is only available when the `yash_syntax` crate is
    /// built with the `annotate-snippets` feature enabled.
    impl From<AnnotationType> for annotate_snippets::Level {
        fn from(r#type: AnnotationType) -> Self {
            use AnnotationType::*;
            match r#type {
                Error => Self::Error,
                Warning => Self::Warning,
                Info => Self::Info,
                Note => Self::Note,
                Help => Self::Help,
            }
        }
    }

    /// Converts `yash_syntax::source::pretty::Message` into
    /// `annotate_snippets::Message`.
    ///
    /// This implementation is only available when the `yash_syntax` crate is
    /// built with the `annotate-snippets` feature enabled.
    impl<'a> From<&'a Message<'a>> for annotate_snippets::Message<'a> {
        fn from(message: &'a Message<'a>) -> Self {
            let mut snippets: Vec<(
                &super::super::Code,
                annotate_snippets::Snippet,
                Vec<annotate_snippets::Annotation>,
            )> = Vec::new();
            // We basically convert each annotation into a snippet, but want to merge annotations
            // with the same code into a single snippet. For this, we first collect all annotations
            // into a temporary vector, and then merge annotations with the same code into a single
            // snippet.
            for annotation in &message.annotations {
                let range = annotation.location.range.clone();
                let level = annotate_snippets::Level::from(annotation.r#type);
                let as_annotation = level.span(range).label(&annotation.label);
                let code = &*annotation.location.code;
                if let Some((_, _, annotations)) =
                    snippets.iter_mut().find(|&&mut (c, _, _)| c == code)
                {
                    annotations.push(as_annotation);
                } else {
                    let line_start = code
                        .start_line_number
                        .get()
                        .try_into()
                        .unwrap_or(usize::MAX);
                    let snippet = annotate_snippets::Snippet::source(&annotation.code)
                        .line_start(line_start)
                        .origin(code.source.label())
                        .fold(true);
                    snippets.push((code, snippet, vec![as_annotation]));
                }
            }

            annotate_snippets::Level::from(message.r#type)
                .title(&message.title)
                .snippets(
                    snippets
                        .into_iter()
                        .map(|(_, snippet, annotations)| snippet.annotations(annotations)),
                )
                .footers(message.footers.iter().map(|footer| {
                    let level = annotate_snippets::Level::from(footer.r#type);
                    level.title(&footer.label)
                }))
        }
    }
}
