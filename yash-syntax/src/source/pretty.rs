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
//! let snippet = annotate_snippets::Snippet::from(&message);
//! eprint!("{}", annotate_snippets::Renderer::plain().render(snippet));
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
    use annotate_snippets::Snippet;

    /// Converts `yash_syntax::source::pretty::AnnotationType` into
    /// `annotate_snippets::snippet::AnnotationType`.
    ///
    /// This implementation is only available when the `yash_syntax` crate is
    /// built with the `annotate-snippets` feature enabled.
    impl From<AnnotationType> for annotate_snippets::AnnotationType {
        fn from(r#type: AnnotationType) -> Self {
            use AnnotationType::*;
            match r#type {
                Error => annotate_snippets::AnnotationType::Error,
                Warning => annotate_snippets::AnnotationType::Warning,
                Info => annotate_snippets::AnnotationType::Info,
                Note => annotate_snippets::AnnotationType::Note,
                Help => annotate_snippets::AnnotationType::Help,
            }
        }
    }

    impl<'a> From<&'a Message<'a>> for Snippet<'a> {
        fn from(message: &'a Message<'a>) -> Self {
            let mut snippet = Snippet {
                title: Some(annotate_snippets::Annotation {
                    id: None,
                    label: Some(&message.title),
                    annotation_type: message.r#type.into(),
                }),
                footer: vec![],
                slices: vec![],
            };

            let mut lines = vec![];
            for annotation in &message.annotations {
                let code = &annotation.location.code;
                let line_start = code
                    .start_line_number
                    .get()
                    .try_into()
                    .unwrap_or(usize::MAX);
                let value = &annotation.code;
                let range = &annotation.location.range;
                let annotation = annotate_snippets::SourceAnnotation {
                    range: (range.start, range.end),
                    label: &annotation.label,
                    annotation_type: annotation.r#type.into(),
                };
                if let Some(i) = lines.iter().position(|l| l == code) {
                    snippet.slices[i].annotations.push(annotation);
                } else {
                    snippet.slices.push(annotate_snippets::Slice {
                        source: value,
                        line_start,
                        origin: Some(code.source.label()),
                        fold: true,
                        annotations: vec![annotation],
                    });
                    lines.push(code.clone());
                }
            }

            for footer in &message.footers {
                snippet.footer.push(annotate_snippets::Annotation {
                    id: None,
                    label: Some(&footer.label),
                    annotation_type: footer.r#type.into(),
                });
            }

            snippet
        }
    }

    #[test]
    fn from_message_type_and_title() {
        let message = Message {
            r#type: AnnotationType::Error,
            title: "my title".into(),
            annotations: vec![],
            footers: vec![],
        };
        let snippet = Snippet::from(&message);
        let title = snippet.title.unwrap();
        assert_eq!(title.id, None);
        assert_eq!(title.label, Some("my title"));
        assert_eq!(
            title.annotation_type,
            annotate_snippets::AnnotationType::Error,
        );
        assert_eq!(snippet.slices.len(), 0, "{:?}", snippet.footer);
        assert_eq!(snippet.footer.len(), 0, "{:?}", snippet.footer);
    }

    #[test]
    fn from_message_one_annotation() {
        let location = Location::dummy("my location");
        let message = Message {
            r#type: AnnotationType::Warning,
            title: "my title".into(),
            annotations: vec![Annotation::new(
                AnnotationType::Info,
                "my label".into(),
                &location,
            )],
            footers: vec![],
        };
        let snippet = Snippet::from(&message);
        let title = snippet.title.unwrap();
        assert_eq!(title.id, None);
        assert_eq!(title.label, Some("my title"));
        assert_eq!(
            title.annotation_type,
            annotate_snippets::AnnotationType::Warning,
        );
        assert_eq!(snippet.slices.len(), 1, "{:?}", snippet.slices);
        assert_eq!(snippet.slices[0].source, "my location");
        assert_eq!(snippet.slices[0].line_start, 1);
        assert_eq!(snippet.slices[0].origin, Some("<?>"));
        assert_eq!(snippet.slices[0].annotations.len(), 1);
        assert_eq!(snippet.slices[0].annotations[0].range, (0, 11));
        assert_eq!(snippet.slices[0].annotations[0].label, "my label");
        assert_eq!(
            snippet.slices[0].annotations[0].annotation_type,
            annotate_snippets::AnnotationType::Info,
        );
        assert_eq!(snippet.footer.len(), 0, "{:?}", snippet.footer);
    }

    #[test]
    fn from_message_non_default_line_start() {
        use super::super::*;
        use std::num::NonZeroU64;
        use std::rc::Rc;

        let location = Location {
            code: Rc::new(Code {
                value: "".to_string().into(),
                start_line_number: NonZeroU64::new(128).unwrap(),
                source: Source::Unknown,
            }),
            range: 42..123,
        };
        let message = Message {
            r#type: AnnotationType::Warning,
            title: "".into(),
            annotations: vec![Annotation::new(AnnotationType::Info, "".into(), &location)],
            footers: vec![],
        };
        let snippet = Snippet::from(&message);
        assert_eq!(snippet.slices[0].line_start, 128);
    }

    #[test]
    fn from_message_non_default_range() {
        let mut location = Location::dummy("my location");
        location.range = 6..9;
        let message = Message {
            r#type: AnnotationType::Warning,
            title: "".into(),
            annotations: vec![Annotation::new(AnnotationType::Info, "".into(), &location)],
            footers: vec![],
        };
        let snippet = Snippet::from(&message);
        assert_eq!(snippet.slices[0].annotations[0].range, (6, 9));
    }

    #[test]
    fn from_message_non_default_origin() {
        use super::super::*;
        use std::num::NonZeroU64;

        let original = Location::dummy("my original");
        let alias = Rc::new(Alias {
            name: "foo".to_string(),
            replacement: "bar".to_string(),
            global: false,
            origin: Location::dummy("my origin"),
        });
        let code = Rc::new(Code {
            value: "substitution".to_string().into(),
            start_line_number: NonZeroU64::new(10).unwrap(),
            source: Source::Alias { original, alias },
        });
        let location = Location { code, range: 4..9 };
        let message = Message {
            r#type: AnnotationType::Warning,
            title: "my title".into(),
            annotations: vec![Annotation::new(
                AnnotationType::Info,
                "my label".into(),
                &location,
            )],
            footers: vec![],
        };
        let snippet = Snippet::from(&message);
        assert_eq!(snippet.slices[0].source, "substitution");
        assert_eq!(snippet.slices[0].line_start, 10);
        assert_eq!(snippet.slices[0].origin, Some("<alias>"));
    }

    #[test]
    fn from_message_two_annotations_different_slice() {
        let location_1 = Location::dummy("my location 1");
        let location_2 = Location::dummy("my location 2");
        let message = Message {
            r#type: AnnotationType::Error,
            title: "some title".into(),
            annotations: vec![
                Annotation::new(AnnotationType::Note, "my label 1".into(), &location_1),
                Annotation::new(AnnotationType::Info, "my label 2".into(), &location_2),
            ],
            footers: vec![],
        };
        let snippet = Snippet::from(&message);
        let title = snippet.title.unwrap();
        assert_eq!(title.id, None);
        assert_eq!(title.label, Some("some title"));
        assert_eq!(
            title.annotation_type,
            annotate_snippets::AnnotationType::Error,
        );
        assert_eq!(snippet.slices.len(), 2, "{:?}", snippet.slices);
        assert_eq!(snippet.slices[0].source, "my location 1");
        assert_eq!(snippet.slices[0].annotations.len(), 1);
        assert_eq!(snippet.slices[0].annotations[0].range, (0, 13));
        assert_eq!(snippet.slices[0].annotations[0].label, "my label 1");
        assert_eq!(
            snippet.slices[0].annotations[0].annotation_type,
            annotate_snippets::AnnotationType::Note,
        );
        assert_eq!(snippet.slices[1].source, "my location 2");
        assert_eq!(snippet.slices[1].annotations.len(), 1);
        assert_eq!(snippet.slices[1].annotations[0].range, (0, 13));
        assert_eq!(snippet.slices[1].annotations[0].label, "my label 2");
        assert_eq!(
            snippet.slices[1].annotations[0].annotation_type,
            annotate_snippets::AnnotationType::Info,
        );
        assert_eq!(snippet.footer.len(), 0, "{:?}", snippet.footer);
    }

    #[test]
    fn from_message_two_annotations_same_slice() {
        let location_1 = Location::dummy("my location");
        let location_3 = Location {
            range: 2..4,
            ..location_1.clone()
        };
        let message = Message {
            r#type: AnnotationType::Error,
            title: "some title".into(),
            annotations: vec![
                Annotation::new(AnnotationType::Info, "my label 1".into(), &location_3),
                Annotation::new(AnnotationType::Help, "my label 2".into(), &location_1),
            ],
            footers: vec![],
        };
        let snippet = Snippet::from(&message);
        let title = snippet.title.unwrap();
        assert_eq!(title.id, None);
        assert_eq!(title.label, Some("some title"));
        assert_eq!(
            title.annotation_type,
            annotate_snippets::AnnotationType::Error,
        );
        assert_eq!(snippet.slices.len(), 1, "{:?}", snippet.slices);
        assert_eq!(snippet.slices[0].source, "my location");
        assert_eq!(snippet.slices[0].annotations.len(), 2);
        assert_eq!(snippet.slices[0].annotations[0].range, (2, 4));
        assert_eq!(snippet.slices[0].annotations[0].label, "my label 1");
        assert_eq!(
            snippet.slices[0].annotations[0].annotation_type,
            annotate_snippets::AnnotationType::Info,
        );
        assert_eq!(snippet.slices[0].annotations[1].range, (0, 11));
        assert_eq!(snippet.slices[0].annotations[1].label, "my label 2");
        assert_eq!(
            snippet.slices[0].annotations[1].annotation_type,
            annotate_snippets::AnnotationType::Help,
        );
        assert_eq!(snippet.footer.len(), 0, "{:?}", snippet.footer);
    }

    #[test]
    fn from_message_one_footer() {
        let message = Message {
            r#type: AnnotationType::Error,
            title: "some title".into(),
            annotations: vec![],
            footers: vec![Footer {
                r#type: AnnotationType::Note,
                label: "footer text".into(),
            }],
        };
        let snippet = Snippet::from(&message);
        let title = snippet.title.unwrap();
        assert_eq!(title.id, None);
        assert_eq!(title.label, Some("some title"));
        assert_eq!(
            title.annotation_type,
            annotate_snippets::AnnotationType::Error,
        );
        assert_eq!(snippet.slices.len(), 0, "{:?}", snippet.slices);
        assert_eq!(snippet.footer.len(), 1, "{:?}", snippet.footer);
        assert_eq!(
            snippet.footer[0].annotation_type,
            annotate_snippets::AnnotationType::Note,
        );
        assert_eq!(snippet.footer[0].label, Some("footer text"));
    }

    #[test]
    fn from_message_many_footers() {
        let message = Message {
            r#type: AnnotationType::Error,
            title: "some title".into(),
            annotations: vec![],
            footers: vec![
                Footer {
                    r#type: AnnotationType::Info,
                    label: "footer 1".into(),
                },
                Footer {
                    r#type: AnnotationType::Warning,
                    label: "footer 2".into(),
                },
                Footer {
                    r#type: AnnotationType::Error,
                    label: "footer 3".into(),
                },
            ],
        };
        let snippet = Snippet::from(&message);
        let title = snippet.title.unwrap();
        assert_eq!(title.id, None);
        assert_eq!(title.label, Some("some title"));
        assert_eq!(
            title.annotation_type,
            annotate_snippets::AnnotationType::Error,
        );
        assert_eq!(snippet.slices.len(), 0, "{:?}", snippet.slices);
        assert_eq!(snippet.footer.len(), 3, "{:?}", snippet.footer);
        assert_eq!(
            snippet.footer[0].annotation_type,
            annotate_snippets::AnnotationType::Info,
        );
        assert_eq!(snippet.footer[0].label, Some("footer 1"));
        assert_eq!(
            snippet.footer[1].annotation_type,
            annotate_snippets::AnnotationType::Warning,
        );
        assert_eq!(snippet.footer[1].label, Some("footer 2"));
        assert_eq!(
            snippet.footer[2].annotation_type,
            annotate_snippets::AnnotationType::Error,
        );
        assert_eq!(snippet.footer[2].label, Some("footer 3"));
    }
}
