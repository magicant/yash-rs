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
//! TODO: Update the documentation for the new API
//!
//! This module defines some data types for constructing intermediate data
//! structures for printing diagnostic messages referencing source code
//! fragments.  When you have an [`Error`](crate::parser::Error), you can
//! convert it to a [`Message`]. Then, you can in turn convert it into
//! `annotate_snippets::Snippet`, for example, and finally format a printable
//! diagnostic message string.
//!
//! When the `yash_syntax` crate is built with the `annotate-snippets` feature
//! enabled, it supports conversion from `Message` to `Group`. If you would
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
//! let group = annotate_snippets::Group::from(&message);
//! eprint!("{}", annotate_snippets::Renderer::plain().render(&[group]));
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
use std::ops::{Deref, Range};
use std::rc::Rc;

/// Type of [`Report`]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReportType {
    #[default]
    None,
    Error,
    Warning,
}

/// Type and label annotating a [`Span`]
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SpanRole<'a> {
    /// Primary span, usually indicating the main cause of a problem
    Primary { label: Cow<'a, str> },
    /// Secondary span, usually indicating related information
    Supplementary { label: Cow<'a, str> },
    // Patch { replacement: Cow<'a, str> },
}

/// Part of source code [`Snippet`] annotated with additional information
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Span<'a> {
    /// Range of bytes in the source code
    pub range: Range<usize>,
    /// Type and label of this span
    pub role: SpanRole<'a>,
}

/// Fragment of source code with annotated spans highlighting specific regions
///
/// A snippet corresponds to a single source [`Code`](super::Code). It contains
/// zero or more [`Span`]s that annotate specific parts of the code.
///
/// `Snippet` holds a [`Ref`] to the string held in `self.code.value`, which
/// provides an access to the string without making a new borrow
/// ([`code_string`](Self::code_string)). This allows creating another
/// message builder such as `annotate_snippets::Snippet` without the need to
/// retain a borrow of `self.code.value`.
#[derive(Debug)]
pub struct Snippet<'a> {
    /// Source code to which the spans refer
    pub code: &'a super::Code,
    /// Reference to the string held in `self.code.value`
    code_string: Ref<'a, str>,
    /// Spans describing parts of the code
    pub spans: Vec<Span<'a>>,
}

impl Snippet<'_> {
    /// Creates a new snippet for the given code without any spans.
    #[must_use]
    pub fn with_code(code: &super::Code) -> Snippet<'_> {
        Self::with_code_and_spans(code, Vec::new())
    }

    /// Creates a new snippet for the given code with the given spans.
    #[must_use]
    pub fn with_code_and_spans<'a>(code: &'a super::Code, spans: Vec<Span<'a>>) -> Snippet<'a> {
        Snippet {
            code,
            code_string: Ref::map(code.value.borrow(), String::as_str),
            spans,
        }
    }

    /// Creates a vector containing a snippet with a primary span.
    ///
    /// This is a convenience function for creating a vector of snippets
    /// containing a primary span created from the given location and label.
    /// The returned vector can be used as the `snippets` field of a
    /// [`Report`].
    ///
    /// This function calls
    /// [`Source::extend_with_context`](super::Source::extend_with_context) for
    /// `location.code.source`, thereby adding supplementary spans describing the
    /// context of the source code. This means that the returned vector may
    /// contain multiple snippets or spans if the source has a related location.
    #[must_use]
    pub fn with_primary_span<'a>(location: &'a Location, label: Cow<'a, str>) -> Vec<Snippet<'a>> {
        let range = location.byte_range();
        let role = SpanRole::Primary { label };
        let spans = vec![Span { range, role }];
        let mut snippets = vec![Snippet::with_code_and_spans(&location.code, spans)];
        location.code.source.extend_with_context(&mut snippets);
        snippets
    }

    /// Returns the string held in `self.code.value`.
    ///
    /// This method returns a reference to the string held in `self.code.value`.
    /// `Snippet` internally holds a `Ref` to the string, which provides an
    /// access to the string without making a new borrow.
    #[inline(always)]
    #[must_use]
    pub fn code_string(&self) -> &str {
        &self.code_string
    }
}

impl Clone for Snippet<'_> {
    fn clone(&self) -> Self {
        Snippet {
            code: &self.code,
            code_string: Ref::clone(&self.code_string),
            spans: self.spans.clone(),
        }
    }
}

impl PartialEq<Snippet<'_>> for Snippet<'_> {
    fn eq(&self, other: &Snippet<'_>) -> bool {
        self.code == other.code && self.spans == other.spans
    }
}

impl Eq for Snippet<'_> {}

/// Type of [`Footnote`]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum FootnoteType {
    /// No specific type
    #[default]
    None,
    // TODO Do we need both Info and Note?
    Info,
    Note,
    /// For footnotes that provide suggestions
    Suggestion,
}

/// Message without associated source code
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Footnote<'a> {
    /// Type of this footnote
    pub r#type: FootnoteType,
    /// Text of this footnote
    pub label: Cow<'a, str>,
}

/// Entire report containing multiple snippets
///
/// `Report` is an intermediate data structure for constructing a diagnostic
/// message. It contains multiple [`Snippet`]s, each of which corresponds to a
/// specific part of the source code being analyzed.
/// See the [module documentation](self) for more details.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Report<'a> {
    /// Type of this report
    pub r#type: ReportType,
    /// Optional identifier of this report (e.g., error code)
    pub id: Option<Cow<'a, str>>,
    /// Main caption of this report
    pub title: Cow<'a, str>,
    /// Source code fragments annotated with additional information
    pub snippets: Vec<Snippet<'a>>,
    /// Additional message without associated source code
    pub footnotes: Vec<Footnote<'a>>,
}

impl Report<'_> {
    /// Creates a new, empty report.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Report::default()
    }
}

/// Type of annotation.
#[deprecated(note = "Use `ReportType` or `FootnoteType` instead", since = "0.16.0")]
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
#[deprecated(note = "Use `Snippet` and `Span` instead", since = "0.16.0")]
#[derive(Clone)]
pub struct Annotation<'a> {
    /// Type of annotation
    #[allow(deprecated)]
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

#[allow(deprecated)]
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

#[allow(deprecated)]
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

/// Additional message without associated source code
#[deprecated(note = "Use `Footnote` instead", since = "0.16.0")]
#[derive(Clone, Debug)]
pub struct Footer<'a> {
    /// Type of this footer
    #[allow(deprecated)]
    pub r#type: AnnotationType,
    /// Text of this footer
    pub label: Cow<'a, str>,
}

/// Entire diagnostic message
#[allow(deprecated)]
#[deprecated(note = "Use `Report` instead", since = "0.16.0")]
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

/// Adds a span to the appropriate snippet in the given vector.
///
/// This is a utility function used in constructing a vector of snippets with
/// annotated spans.
///
/// If a snippet for the given code already exists in the vector, this function
/// adds the span to that snippet. Otherwise, it creates a new snippet with the
/// given code and span, and appends it to the vector.
pub fn add_span<'a>(code: &'a super::Code, span: Span<'a>, snippets: &mut Vec<Snippet<'a>>) {
    if let Some(snippet) = snippets.iter_mut().find(|s| std::ptr::eq(s.code, code)) {
        snippet.spans.push(span);
    } else {
        snippets.push(Snippet::with_code_and_spans(code, vec![span]));
    }
}

#[test]
fn test_add_span_with_matching_code() {
    let code = Rc::new(super::Code {
        value: std::cell::RefCell::new("echo hello".to_string()),
        start_line_number: std::num::NonZero::new(1).unwrap(),
        source: Rc::new(super::Source::CommandString),
    });
    let span = Span {
        range: 5..10,
        role: SpanRole::Primary {
            label: "greeting".into(),
        },
    };
    let mut snippets = vec![Snippet::with_code(&code)];

    add_span(&code, span, &mut snippets);

    assert_eq!(snippets.len(), 1);
    assert_eq!(snippets[0].spans.len(), 1);
    assert_eq!(snippets[0].spans[0].range, 5..10);
    assert_eq!(
        snippets[0].spans[0].role,
        SpanRole::Primary {
            label: "greeting".into()
        }
    );
}

#[test]
fn test_add_span_without_matching_code() {
    let code1 = Rc::new(super::Code {
        value: std::cell::RefCell::new("echo hello".to_string()),
        start_line_number: std::num::NonZero::new(1).unwrap(),
        source: Rc::new(super::Source::CommandString),
    });
    let code2 = Rc::new(super::Code {
        value: std::cell::RefCell::new("ls -l".to_string()),
        start_line_number: std::num::NonZero::new(1).unwrap(),
        source: Rc::new(super::Source::CommandString),
    });
    let span = Span {
        range: 0..2,
        role: SpanRole::Primary {
            label: "list".into(),
        },
    };
    let mut snippets = vec![Snippet::with_code(&code1)];

    add_span(&code2, span, &mut snippets);

    assert_eq!(snippets.len(), 2);
    assert_eq!(snippets[0].code.value.borrow().as_str(), "echo hello");
    assert_eq!(snippets[0].spans.len(), 0);
    assert_eq!(snippets[1].code.value.borrow().as_str(), "ls -l");
    assert_eq!(snippets[1].spans.len(), 1);
    assert_eq!(snippets[1].spans[0].range, 0..2);
    assert_eq!(
        snippets[1].spans[0].role,
        SpanRole::Primary {
            label: "list".into()
        }
    );
}

impl super::Source {
    /// Extends the given vector of snippets with spans annotating the context of this source.
    ///
    /// If `self` is a source that has a related location (e.g., the `original` field of
    /// `CommandSubst`), this method adds one or more spans describing the location to the given
    /// vector. If the `code` of the location is already present in the vector, it adds the span
    /// to the existing snippet; otherwise, it creates a new snippet.
    ///
    /// If `self` does not have a related location, this method does nothing.
    pub fn extend_with_context<'a>(&'a self, snippets: &mut Vec<Snippet<'a>>) {
        use super::Source::*;
        match self {
            Unknown
            | Stdin
            | CommandString
            | CommandFile { .. }
            | VariableValue { .. }
            | InitFile { .. }
            | Other { .. } => (),

            CommandSubst { original } => {
                let range = original.byte_range();
                let role = SpanRole::Supplementary {
                    label: "command substitution appeared here".into(),
                };
                add_span(&original.code, Span { range, role }, snippets);
            }

            Arith { original } => {
                let range = original.byte_range();
                let role = SpanRole::Supplementary {
                    label: "arithmetic expansion appeared here".into(),
                };
                add_span(&original.code, Span { range, role }, snippets);
            }

            Eval { original } => {
                let range = original.byte_range();
                let role = SpanRole::Supplementary {
                    label: "command passed to the eval built-in here".into(),
                };
                add_span(&original.code, Span { range, role }, snippets);
            }

            DotScript { name, origin } => {
                let range = origin.byte_range();
                let role = SpanRole::Supplementary {
                    label: format!("script `{name}` was sourced here").into(),
                };
                add_span(&origin.code, Span { range, role }, snippets);
            }

            Trap { origin, .. } => {
                let range = origin.byte_range();
                let role = SpanRole::Supplementary {
                    label: "trap was set here".into(),
                };
                add_span(&origin.code, Span { range, role }, snippets);
            }

            Alias { original, alias } => {
                // Where the alias was substituted
                let range = original.byte_range();
                let role = SpanRole::Supplementary {
                    label: format!("alias `{}` was substituted here", alias.name).into(),
                };
                add_span(&original.code, Span { range, role }, snippets);
                // Recurse into the source of the substituted code
                original.code.source.extend_with_context(snippets);

                // Where the alias was defined
                let range = alias.origin.byte_range();
                let role = SpanRole::Supplementary {
                    label: format!("alias `{}` was defined here", alias.name).into(),
                };
                add_span(&alias.origin.code, Span { range, role }, snippets);
                // Recurse into the source of the alias definition
                alias.origin.code.source.extend_with_context(snippets);
            }
        }
    }

    /// Appends complementary annotations describing this source.
    #[allow(deprecated)]
    #[deprecated(note = "Use `extend_with_context` instead", since = "0.16.0")]
    pub fn complement_annotations<'a, 's: 'a, T: Extend<Annotation<'a>>>(&'s self, result: &mut T) {
        use super::Source::*;
        match self {
            Unknown
            | Stdin
            | CommandString
            | CommandFile { .. }
            | VariableValue { .. }
            | InitFile { .. }
            | Other { .. } => (),

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
#[allow(deprecated)]
#[deprecated(note = "Use `Report` instead", since = "0.16.0")]
pub trait MessageBase {
    /// Returns the type of the entire message.
    ///
    /// The default implementation returns `AnnotationType::Error`.
    fn message_type(&self) -> AnnotationType {
        AnnotationType::Error
    }

    // TODO message tag

    /// Returns the main caption of the message.
    fn message_title(&self) -> Cow<'_, str>;

    /// Returns an annotation to be the first in the message.
    fn main_annotation(&self) -> Annotation<'_>;

    /// Adds additional annotations to the given container.
    ///
    /// The default implementation does nothing.
    fn additional_annotations<'a, T: Extend<Annotation<'a>>>(&'a self, results: &mut T) {
        let _ = results;
    }

    /// Returns footers that are included in the message.
    fn footers(&self) -> Vec<Footer<'_>> {
        Vec::new()
    }
}

/// Constructs a message based on the message base.
#[allow(deprecated)]
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

    impl From<ReportType> for annotate_snippets::Level<'_> {
        fn from(r#type: ReportType) -> Self {
            use ReportType::*;
            match r#type {
                None => Self::INFO.no_name(),
                Error => Self::ERROR,
                Warning => Self::WARNING,
            }
        }
    }

    /// Converts `yash_syntax::source::pretty::Span` into
    /// `annotate_snippets::Annotation`.
    ///
    /// This conversion is not provided as a public `From<&Span> for Annotation` implementation
    /// because a future variant of `SpanRole` may map to
    /// `annotate_snippets::Patch` instead of `annotate_snippets::Annotation`.
    fn span_to_annotation<'a>(span: &'a Span<'a>) -> annotate_snippets::Annotation<'a> {
        use annotate_snippets::AnnotationKind as AK;
        let (kind, label) = match &span.role {
            SpanRole::Primary { label } => (AK::Primary, label),
            SpanRole::Supplementary { label } => (AK::Context, label),
        };
        kind.span(span.range.clone()).label(label)
    }

    // `From<&Snippet>` is not implemented for
    // `annotate_snippets::Snippet<'_, annotate_snippets::Annotation<'_>>`
    // because a future variant of `SpanRole` may map to
    // `annotate_snippets::Patch` instead of `annotate_snippets::Annotation`.

    /// Converts `yash_syntax::source::pretty::Snippet` into
    /// `annotate_snippets::Snippet<'a, annotate_snippets::Annotation<'a>>`.
    ///
    /// This conversion is not provided as a public `From<&Snippet> for Snippet` implementation
    /// because a future variant of `SpanRole` may map to
    /// `annotate_snippets::Patch` instead of `annotate_snippets::Annotation`, which does not fit
    /// into a single `annotate_snippets::Snippet<'a, annotate_snippets::Annotation<'a>>`.
    fn snippet_to_annotation_snippet<'a>(
        snippet: &'a Snippet<'a>,
    ) -> annotate_snippets::Snippet<'a, annotate_snippets::Annotation<'a>> {
        annotate_snippets::Snippet::source(snippet.code_string())
            .line_start(
                snippet
                    .code
                    .start_line_number
                    .get()
                    .try_into()
                    .unwrap_or(usize::MAX),
            )
            .path(snippet.code.source.label())
            .annotations(snippet.spans.iter().map(span_to_annotation))
    }

    /// Converts `yash_syntax::source::pretty::FootnoteType` into
    /// `annotate_snippets::Level`.
    ///
    /// This implementation is only available when the `yash_syntax` crate is
    /// built with the `annotate-snippets` feature enabled.
    impl From<FootnoteType> for annotate_snippets::Level<'_> {
        fn from(r#type: FootnoteType) -> Self {
            use FootnoteType::*;
            match r#type {
                None => Self::INFO.no_name(),
                Info => Self::INFO,
                Note => Self::NOTE,
                Suggestion => Self::HELP,
            }
        }
    }

    /// Converts `yash_syntax::source::pretty::Footnote` into
    /// `annotate_snippets::Message`.
    ///
    /// This implementation is only available when the `yash_syntax` crate is
    /// built with the `annotate-snippets` feature enabled.
    impl<'a> From<Footnote<'a>> for annotate_snippets::Message<'a> {
        fn from(footer: Footnote<'a>) -> Self {
            annotate_snippets::Level::from(footer.r#type).message(footer.label)
        }
    }

    /// Converts `&yash_syntax::source::pretty::Footnote` into
    /// `annotate_snippets::Message`.
    ///
    /// This implementation is only available when the `yash_syntax` crate is
    /// built with the `annotate-snippets` feature enabled.
    impl<'a> From<&'a Footnote<'a>> for annotate_snippets::Message<'a> {
        fn from(footer: &'a Footnote<'a>) -> Self {
            annotate_snippets::Level::from(footer.r#type).message(&*footer.label)
        }
    }

    /// Converts `yash_syntax::source::pretty::Report` into
    /// `annotate_snippets::Group`.
    ///
    /// This implementation is only available when the `yash_syntax` crate is
    /// built with the `annotate-snippets` feature enabled.
    impl<'a> From<&'a Report<'a>> for annotate_snippets::Group<'a> {
        fn from(report: &'a Report<'a>) -> Self {
            let title = annotate_snippets::Level::from(report.r#type).primary_title(&*report.title);
            let title = if let Some(id) = &report.id {
                title.id(&**id)
            } else {
                title
            };

            title
                .elements(report.snippets.iter().map(snippet_to_annotation_snippet))
                .elements(
                    report
                        .footnotes
                        .iter()
                        .map(annotate_snippets::Message::from),
                )
        }
    }

    /// Converts `yash_syntax::source::pretty::AnnotationType` into
    /// `annotate_snippets::Level`.
    ///
    /// This implementation is only available when the `yash_syntax` crate is
    /// built with the `annotate-snippets` feature enabled.
    #[allow(deprecated)]
    impl<'a> From<AnnotationType> for annotate_snippets::Level<'a> {
        fn from(r#type: AnnotationType) -> Self {
            use AnnotationType::*;
            match r#type {
                Error => Self::ERROR,
                Warning => Self::WARNING,
                Info => Self::INFO,
                Note => Self::NOTE,
                Help => Self::HELP,
            }
        }
    }

    /// Converts `yash_syntax::source::pretty::AnnotationType` into
    /// `annotate_snippets::AnnotationKind`.
    ///
    /// This implementation is only available when the `yash_syntax` crate is
    /// built with the `annotate-snippets` feature enabled.
    #[allow(deprecated)]
    impl From<AnnotationType> for annotate_snippets::AnnotationKind {
        fn from(r#type: AnnotationType) -> Self {
            use AnnotationType::*;
            match r#type {
                Error | Warning => Self::Primary,
                Info | Note | Help => Self::Context,
            }
        }
    }

    /// Converts `yash_syntax::source::pretty::Message` into
    /// `annotate_snippets::Group`.
    ///
    /// This implementation is only available when the `yash_syntax` crate is
    /// built with the `annotate-snippets` feature enabled.
    #[allow(deprecated)]
    impl<'a> From<&'a Message<'a>> for annotate_snippets::Group<'a> {
        fn from(message: &'a Message<'a>) -> Self {
            let mut snippets: Vec<(
                &super::super::Code,
                annotate_snippets::Snippet<'a, annotate_snippets::Annotation<'a>>,
                Vec<annotate_snippets::Annotation>,
            )> = Vec::new();
            // We basically convert each annotation into a snippet, but want to merge annotations
            // with the same code into a single snippet. For this, we first collect all annotations
            // into a temporary vector, and then merge annotations with the same code into a single
            // snippet.
            for annotation in &message.annotations {
                let range = annotation.location.byte_range();
                let as_annotation = annotate_snippets::AnnotationKind::from(annotation.r#type)
                    .span(range)
                    .label(&annotation.label);
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
                    let snippet = annotate_snippets::Snippet::source(&**annotation.code)
                        .line_start(line_start)
                        .path(code.source.label());
                    snippets.push((code, snippet, vec![as_annotation]));
                }
            }

            annotate_snippets::Level::from(message.r#type)
                .primary_title(&*message.title)
                .elements(
                    snippets
                        .into_iter()
                        .map(|(_, snippet, annotations)| snippet.annotations(annotations)),
                )
                .elements(message.footers.iter().map(|footer| {
                    annotate_snippets::Level::from(footer.r#type).message(&*footer.label)
                }))
        }
    }
}
