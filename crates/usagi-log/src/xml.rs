//! Just enough XML to read mjlog files.
//!
//! An mjlog is a flat list of elements like `<T25/>` or
//! `<INIT seed="0,0,0,5,4,105" ... />` inside one `<mjloggm>` root. There
//! is no text content, no nesting beyond the root and no entities that
//! matter, so a small tokenizer is all it takes and keeps the crate free of
//! an XML dependency.

use std::fmt;

/// One element: its name and its attributes in file order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element<'a> {
    pub name: &'a str,
    pub attrs: Vec<(&'a str, &'a str)>,
}

impl<'a> Element<'a> {
    pub fn attr(&self, key: &str) -> Option<&'a str> {
        self.attrs.iter().find(|(k, _)| *k == key).map(|&(_, v)| v)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlError {
    /// Byte offset into the input.
    pub offset: usize,
    pub message: &'static str,
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for XmlError {}

/// Every opening (or self-closing) element in `input`, in order. Closing
/// tags, comments and the `<?xml ...?>` declaration are skipped.
pub fn elements(input: &str) -> Result<Vec<Element<'_>>, XmlError> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = input[i..].find('<') {
        let start = i + off;
        let err = |message| XmlError {
            offset: start,
            message,
        };
        let rest = &input[start + 1..];
        if rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('!') {
            let end = rest.find('>').ok_or(err("unterminated tag"))?;
            i = start + 1 + end + 1;
            continue;
        }
        let mut j = start + 1;
        while j < bytes.len() && !matches!(bytes[j], b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>') {
            j += 1;
        }
        let name = &input[start + 1..j];
        if name.is_empty() {
            return Err(err("empty element name"));
        }
        let mut attrs = Vec::new();
        loop {
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            match bytes.get(j) {
                None => return Err(err("unterminated tag")),
                Some(b'>') => {
                    j += 1;
                    break;
                }
                Some(b'/') => {
                    if bytes.get(j + 1) != Some(&b'>') {
                        return Err(err("stray '/' in tag"));
                    }
                    j += 2;
                    break;
                }
                Some(_) => {
                    let key_start = j;
                    while j < bytes.len() && bytes[j] != b'=' {
                        if bytes[j].is_ascii_whitespace() || bytes[j] == b'>' {
                            return Err(err("attribute without a value"));
                        }
                        j += 1;
                    }
                    let key = &input[key_start..j];
                    let quote = *bytes.get(j + 1).ok_or(err("unterminated tag"))?;
                    if quote != b'"' && quote != b'\'' {
                        return Err(err("unquoted attribute value"));
                    }
                    let value_start = j + 2;
                    let len = input[value_start..]
                        .find(quote as char)
                        .ok_or(err("unterminated attribute value"))?;
                    attrs.push((key, &input[value_start..value_start + len]));
                    j = value_start + len + 1;
                }
            }
        }
        out.push(Element { name, attrs });
        i = j;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_elements_and_attributes() {
        let els = elements(
            r#"<?xml version="1.0"?><mjloggm ver="2.3"><T25/><N who="0" m="49259" /></mjloggm>"#,
        )
        .unwrap();
        assert_eq!(els.len(), 3);
        assert_eq!(els[0].name, "mjloggm");
        assert_eq!(els[1].name, "T25");
        assert!(els[1].attrs.is_empty());
        assert_eq!(els[2].attr("who"), Some("0"));
        assert_eq!(els[2].attr("m"), Some("49259"));
        assert_eq!(els[2].attr("x"), None);
    }

    #[test]
    fn rejects_broken_input() {
        assert!(elements("<T25").is_err());
        assert!(elements(r#"<N who=0/>"#).is_err());
        assert!(elements(r#"<N who="0/>"#).is_err());
    }
}
