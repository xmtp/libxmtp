use std::collections::HashMap;

use anyhow::{Result, bail};
use pest::iterators::Pair;

use crate::Rule;

#[derive(Debug, PartialEq, Eq)]
pub enum Value {
    String(String),
    Int(i64),
    Object(HashMap<String, Value>),
    Array(Vec<Self>),
    Boolean(bool),
    None,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_indented(f, 0)
    }
}

impl Value {
    fn fmt_indented(&self, f: &mut std::fmt::Formatter<'_>, indent: usize) -> std::fmt::Result {
        match self {
            Value::String(s) => write!(f, "{}", s),
            Value::Int(i) => write!(f, "{}", i),
            Value::Object(obj) => {
                if obj.is_empty() {
                    return write!(f, "{{}}");
                }
                let indent_str = "  ".repeat(indent);
                let inner_indent = "  ".repeat(indent + 1);
                writeln!(f, "{{")?;
                let mut first = true;
                for (k, v) in obj.iter() {
                    if !first {
                        writeln!(f, ",")?;
                    }
                    first = false;
                    write!(f, "{}{}: ", inner_indent, k)?;
                    v.fmt_indented(f, indent + 1)?;
                }
                writeln!(f)?;
                write!(f, "{}}}", indent_str)
            }
            Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Value::Boolean(b) => write!(f, "{}", b),
            Value::None => write!(f, "null"),
        }
    }

    pub fn from(pair: Pair<'_, Rule>) -> Result<Self> {
        let pair_str = pair.as_str();
        let val = match pair.as_rule() {
            Rule::quoted_string => Self::String(pair_str.replace("\"", "").to_string()),
            Rule::number => Self::Int(pair_str.parse()?),
            Rule::array | Rule::braced_array => {
                let mut array = Vec::new();
                for item in pair.into_inner() {
                    if let Ok(item) = Value::from(item) {
                        array.push(item);
                    }
                }
                Self::Array(array)
            }
            Rule::object => {
                let mut object = HashMap::new();
                for pair in pair.into_inner() {
                    let mut pair_inner = pair.into_inner();
                    let Some(key) = pair_inner.next() else {
                        continue;
                    };
                    let Some(value) = pair_inner.next() else {
                        continue;
                    };

                    // Do this so we don't completely omit the line if a single value fails to parse.
                    if let Ok(value) = Self::from(value) {
                        object.insert(key.as_str().replace("\"", "").to_string(), value);
                    }
                }
                Self::Object(object)
            }
            Rule::boolean => match pair_str {
                "true" => Self::Boolean(true),
                "false" => Self::Boolean(false),
                _ => unreachable!(),
            },
            Rule::null => Self::None,
            Rule::unquoted_string | Rule::option_value | Rule::option_inner => {
                Self::String(pair_str.to_string())
            }
            _ => bail!("Unsupportd rule encountered while parsing context."),
        };

        Ok(val)
    }

    pub fn as_str(&self) -> Result<&str> {
        match self {
            Self::String(str) => Ok(&str),
            _ => bail!("Downcasting failed. {self:?} is not a string."),
        }
    }

    pub fn as_int(&self) -> Result<i64> {
        match self {
            Self::Int(int) => Ok(*int),
            _ => bail!("Downcasting failed. {self:?} is not an int."),
        }
    }

    pub fn as_obj(&self) -> Result<&HashMap<String, Self>> {
        match self {
            Self::Object(obj) => Ok(obj),
            _ => bail!("Downcasting failed. {self:?} is not an obj."),
        }
    }
}
