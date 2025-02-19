// copy-paste of https://docs.rs/tracing-forest/latest/src/tracing_forest/printer/pretty.rs.html#62
// but with slight variations
use std::fmt::{self, Write};
use tracing_forest::printer::Formatter;
use tracing_forest::tree::{Event, Span, Tree};

type IndentVec = Vec<Indent>;

pub struct Contextual;
impl Contextual {
    fn format_tree(
        tree: &Tree,
        duration_root: Option<f64>,
        indent: &mut IndentVec,
        writer: &mut String,
    ) -> fmt::Result {
        match tree {
            Tree::Event(event) => {
                // write!(writer, "{:36} ", event.timestamp().to_rfc3339())?;
                // write!(writer, "{:<8} ", event.level())?;
                Contextual::format_indent(indent, writer)?;
                Contextual::format_event(event, writer)
            }
            Tree::Span(span) => {
                // write!(writer, "{:36} ", span.timestamp().to_rfc3339())?;
                // write!(writer, "{:<8} ", span.level())?;
                Contextual::format_indent(indent, writer)?;
                Contextual::format_span(span, duration_root, indent, writer)
            }
        }
    }

    fn format_indent(indent: &[Indent], writer: &mut String) -> fmt::Result {
        for indent in indent {
            writer.write_str(indent.repr())?;
        }
        Ok(())
    }

    fn format_event(event: &Event, writer: &mut String) -> fmt::Result {
        let mut message = String::new();
        if let Some(msg) = event.message() {
            message = message + msg;
            let ids = super::REPLACE_IDS.lock();
            for (id, name) in ids.iter() {
                message = message.replace(id, name);
            }
            writer.write_str(&message)?;
        }
        /*
                    for field in event.fields().iter() {
                        write!(writer, " | {}: {}", field.key(), field.value())?;
                    }
        */
        writeln!(writer)
    }

    fn format_span(
        span: &Span,
        duration_root: Option<f64>,
        indent: &mut IndentVec,
        writer: &mut String,
    ) -> fmt::Result {
        let total_duration = span.total_duration().as_nanos() as f64;
        let inner_duration = span.inner_duration().as_nanos() as f64;
        let root_duration = duration_root.unwrap_or(total_duration);
        let percent_total_of_root_duration = 100.0 * total_duration / root_duration;

        write!(
            writer,
            "{} [ {} | ",
            span.name(),
            DurationDisplay(total_duration)
        )?;

        if inner_duration > 0.0 {
            let base_duration = span.base_duration().as_nanos() as f64;
            let percent_base_of_root_duration = 100.0 * base_duration / root_duration;
            write!(writer, "{:.2}% / ", percent_base_of_root_duration)?;
        }

        write!(writer, "{:.2}% ]", percent_total_of_root_duration)?;
        /*
                    for (n, field) in span.shared.fields.iter().enumerate() {
                        write!(
                            writer,
                            "{} {}: {}",
                            if n == 0 { "" } else { " |" },
                            field.key(),
                            field.value()
                        )?;
                    }
        */
        writeln!(writer)?;

        if let Some((last, remaining)) = span.nodes().split_last() {
            match indent.last_mut() {
                Some(edge @ Indent::Turn) => *edge = Indent::Null,
                Some(edge @ Indent::Fork) => *edge = Indent::Line,
                _ => {}
            }

            indent.push(Indent::Fork);

            for tree in remaining {
                if let Some(edge) = indent.last_mut() {
                    *edge = Indent::Fork;
                }
                Contextual::format_tree(tree, Some(root_duration), indent, writer)?;
            }

            if let Some(edge) = indent.last_mut() {
                *edge = Indent::Turn;
            }
            Contextual::format_tree(last, Some(root_duration), indent, writer)?;

            indent.pop();
        }

        Ok(())
    }
}
impl Formatter for Contextual {
    type Error = std::fmt::Error;

    fn fmt(&self, tree: &Tree) -> Result<String, Self::Error> {
        let mut writer = String::with_capacity(256);
        Contextual::format_tree(tree, None, &mut IndentVec::new(), &mut writer)?;
        Ok(writer)
    }
}

enum Indent {
    Null,
    Line,
    Fork,
    Turn,
}

impl Indent {
    fn repr(&self) -> &'static str {
        match self {
            Self::Null => "   ",
            Self::Line => "│  ",
            Self::Fork => "┝━ ",
            Self::Turn => "┕━ ",
        }
    }
}

struct DurationDisplay(f64);

// Taken from chrono
impl fmt::Display for DurationDisplay {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut t = self.0;
        for unit in ["ns", "µs", "ms", "s"] {
            if t < 10.0 {
                return write!(f, "{:.2}{}", t, unit);
            } else if t < 100.0 {
                return write!(f, "{:.1}{}", t, unit);
            } else if t < 1000.0 {
                return write!(f, "{:.0}{}", t, unit);
            }
            t /= 1000.0;
        }
        write!(f, "{:.0}s", t * 1000.0)
    }
}
