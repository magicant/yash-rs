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

//! Pretty-printing diagnostic messages containing references to source code.
//!
//! This module defines some data types for constructing intermediate data
//! structures for printing diagnostic messages referencing source code
//! fragments.  When you have an [`Error`](crate::parser::Error), you can
//! convert it to a [`Message`]. Then, you can in turn convert it into
//! `annotate_snippets::snippet::Snippet`, for example, and finally format a
//! printable diagnostic message string.
//!
//! When the `yash_syntax` crate is built with the `annotate-snippets` feature
//! enabled, it supports conversion from `Message` to `Snippet`. If you would
//! like to use another formatter instead, you can provide your own conversion
//! for yourself.
//!
//! TODO Elaborate

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

/// Source code fragment annotated with a label.
///
/// Annotations are part of an entire [`Message`].
#[derive(Clone)]
pub struct Annotation<'a> {
    /// Type of annotation.
    pub r#type: AnnotationType,
    /// String that describes the annotated part of the source code.
    pub label: Cow<'a, str>,
    /// Position of the annotated fragment in the source code.
    pub location: &'a Location,
    /// Annotated code string.
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
    pub fn new(r#type: AnnotationType, label: Cow<'a, str>, location: &'a Location) -> Self {
        Annotation {
            r#type,
            label,
            location,
            code: Rc::new(Ref::map(location.code.value.borrow(), String::as_str)),
        }
    }
}

/// Entire diagnostic message.
#[derive(Clone, Debug)]
pub struct Message<'a> {
    /// Type of this message.
    pub r#type: AnnotationType,
    /// String that communicates the most important information in this message.
    pub title: Cow<'a, str>,
    /// References to source code fragments annotated with additional information.
    pub annotations: Vec<Annotation<'a>>,
}

impl super::Source {
    /// Appends complementary annotations describing this source.
    pub fn complement_annotations<'a, 's: 'a, T: Extend<Annotation<'a>>>(&'s self, result: &mut T) {
        use super::Source::*;
        match self {
            Unknown | Stdin => (),
            CommandSubst { original: _ } => {
                // TODO Use Extend::extend_one
                // FIXME
                // result.extend(std::iter::once(Annotation::new(
                //     AnnotationType::Info,
                //     "command substitution appeared here".into(),
                //     original,
                // )));
            }
            Trap { origin: _, .. } => {
                // TODO Use Extend::extend_one
                // FIXME
                // result.extend(std::iter::once(Annotation::new(
                //     AnnotationType::Info,
                //     "trap was set here".into(),
                //     origin,
                // )));
            }
            Alias { original, alias } => {
                // TODO Use Extend::extend_one
                // FIXME
                // result.extend(std::iter::once(Annotation::new(
                //     AnnotationType::Info,
                //     format!("alias `{}` was substituted here", alias.name).into(),
                //     original,
                // )));
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

#[cfg(feature = "annotate-snippets")]
mod annotate_snippets_support {
    use super::*;
    use annotate_snippets::snippet;
    use annotate_snippets::snippet::Snippet;

    /// Converts `yash_syntax::source::pretty::AnnotationType` into
    /// `annotate_snippets::snippet::AnnotationType`.
    ///
    /// This implementation is only available when the `yash_syntax` crate is
    /// built with the `annotate-snippets` feature enabled.
    impl From<AnnotationType> for annotate_snippets::snippet::AnnotationType {
        fn from(r#type: AnnotationType) -> Self {
            use AnnotationType::*;
            match r#type {
                Error => snippet::AnnotationType::Error,
                Warning => snippet::AnnotationType::Warning,
                Info => snippet::AnnotationType::Info,
                Note => snippet::AnnotationType::Note,
                Help => snippet::AnnotationType::Help,
            }
        }
    }

    impl<'a> From<&'a Message<'a>> for Snippet<'a> {
        fn from(message: &'a Message<'a>) -> Self {
            let mut snippet = Snippet {
                title: Some(snippet::Annotation {
                    id: None,
                    label: Some(&message.title),
                    annotation_type: message.r#type.into(),
                }),
                footer: vec![],
                slices: vec![],
                opt: annotate_snippets::display_list::FormatOptions::default(),
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
                let index = annotation.location.index;
                let index = index.min(value.chars().count().saturating_sub(1));
                let annotation = snippet::SourceAnnotation {
                    range: (index, index + 1),
                    label: &annotation.label,
                    annotation_type: annotation.r#type.into(),
                };
                if let Some(i) = lines.iter().position(|l| l == code) {
                    snippet.slices[i].annotations.push(annotation);
                } else {
                    snippet.slices.push(snippet::Slice {
                        source: value,
                        line_start,
                        origin: Some(code.source.label()),
                        fold: true,
                        annotations: vec![annotation],
                    });
                    lines.push(code.clone());
                }
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
        };
        let snippet = Snippet::from(&message);
        let title = snippet.title.unwrap();
        assert_eq!(title.id, None);
        assert_eq!(title.label, Some("my title"));
        assert_eq!(title.annotation_type, snippet::AnnotationType::Error);
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
        };
        let snippet = Snippet::from(&message);
        let title = snippet.title.unwrap();
        assert_eq!(title.id, None);
        assert_eq!(title.label, Some("my title"));
        assert_eq!(title.annotation_type, snippet::AnnotationType::Warning);
        assert_eq!(snippet.slices.len(), 1, "{:?}", snippet.slices);
        assert_eq!(snippet.slices[0].source, "my location");
        assert_eq!(snippet.slices[0].line_start, 1);
        assert_eq!(snippet.slices[0].origin, Some("<?>"));
        assert_eq!(snippet.slices[0].annotations.len(), 1);
        assert_eq!(snippet.slices[0].annotations[0].range, (0, 1));
        assert_eq!(snippet.slices[0].annotations[0].label, "my label");
        assert_eq!(
            snippet.slices[0].annotations[0].annotation_type,
            snippet::AnnotationType::Info
        );
    }

    #[test]
    fn from_message_non_default_line_start() {
        use super::super::*;
        use std::num::NonZeroU64;
        use std::rc::Rc;

        let code = Rc::new(Code {
            value: "".to_string().into(),
            start_line_number: NonZeroU64::new(128).unwrap(),
            source: Source::Unknown,
        });
        let location = Location { code, index: 42 };
        let message = Message {
            r#type: AnnotationType::Warning,
            title: "".into(),
            annotations: vec![Annotation::new(AnnotationType::Info, "".into(), &location)],
        };
        let snippet = Snippet::from(&message);
        assert_eq!(snippet.slices[0].line_start, 128);
    }

    #[test]
    fn from_message_non_default_range() {
        let mut location = Location::dummy("my location");
        location.index = 6;
        let message = Message {
            r#type: AnnotationType::Warning,
            title: "".into(),
            annotations: vec![Annotation::new(AnnotationType::Info, "".into(), &location)],
        };
        let snippet = Snippet::from(&message);
        assert_eq!(snippet.slices[0].annotations[0].range, (6, 7));
    }

    #[test]
    fn from_message_range_overflow() {
        let mut location = Location::dummy("my location");
        location.index = 11;
        let message = Message {
            r#type: AnnotationType::Warning,
            title: "".into(),
            annotations: vec![Annotation::new(AnnotationType::Info, "".into(), &location)],
        };
        let snippet = Snippet::from(&message);
        assert_eq!(snippet.slices[0].annotations[0].range, (10, 11));
    }

    #[test]
    fn from_message_non_default_origin() {
        use super::super::*;
        use std::num::NonZeroU64;

        let original = Span::dummy("my original");
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
        let location = Location { code, index: 4 };
        let message = Message {
            r#type: AnnotationType::Warning,
            title: "my title".into(),
            annotations: vec![Annotation::new(
                AnnotationType::Info,
                "my label".into(),
                &location,
            )],
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
        };
        let snippet = Snippet::from(&message);
        let title = snippet.title.unwrap();
        assert_eq!(title.id, None);
        assert_eq!(title.label, Some("some title"));
        assert_eq!(title.annotation_type, snippet::AnnotationType::Error);
        assert_eq!(snippet.slices.len(), 2, "{:?}", snippet.slices);
        assert_eq!(snippet.slices[0].source, "my location 1");
        assert_eq!(snippet.slices[0].annotations.len(), 1);
        assert_eq!(snippet.slices[0].annotations[0].range, (0, 1));
        assert_eq!(snippet.slices[0].annotations[0].label, "my label 1");
        assert_eq!(
            snippet.slices[0].annotations[0].annotation_type,
            snippet::AnnotationType::Note
        );
        assert_eq!(snippet.slices[1].source, "my location 2");
        assert_eq!(snippet.slices[1].annotations.len(), 1);
        assert_eq!(snippet.slices[1].annotations[0].range, (0, 1));
        assert_eq!(snippet.slices[1].annotations[0].label, "my label 2");
        assert_eq!(
            snippet.slices[1].annotations[0].annotation_type,
            snippet::AnnotationType::Info
        );
    }

    #[test]
    fn from_message_two_annotations_same_slice() {
        let location_1 = Location::dummy("my location");
        let location_3 = Location {
            index: 2,
            ..location_1.clone()
        };
        let message = Message {
            r#type: AnnotationType::Error,
            title: "some title".into(),
            annotations: vec![
                Annotation::new(AnnotationType::Info, "my label 1".into(), &location_3),
                Annotation::new(AnnotationType::Help, "my label 2".into(), &location_1),
            ],
        };
        let snippet = Snippet::from(&message);
        let title = snippet.title.unwrap();
        assert_eq!(title.id, None);
        assert_eq!(title.label, Some("some title"));
        assert_eq!(title.annotation_type, snippet::AnnotationType::Error);
        assert_eq!(snippet.slices.len(), 1, "{:?}", snippet.slices);
        assert_eq!(snippet.slices[0].source, "my location");
        assert_eq!(snippet.slices[0].annotations.len(), 2);
        assert_eq!(snippet.slices[0].annotations[0].range, (2, 3));
        assert_eq!(snippet.slices[0].annotations[0].label, "my label 1");
        assert_eq!(
            snippet.slices[0].annotations[0].annotation_type,
            snippet::AnnotationType::Info
        );
        assert_eq!(snippet.slices[0].annotations[1].range, (0, 1));
        assert_eq!(snippet.slices[0].annotations[1].label, "my label 2");
        assert_eq!(
            snippet.slices[0].annotations[1].annotation_type,
            snippet::AnnotationType::Help
        );
    }
}
