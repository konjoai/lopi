use super::{
    decode_key, decode_primitive, parse_key_rest, split_on_delim, try_parse_header, DecoderOptions,
    Header, Line, ToonError,
};
use serde_json::{Map, Value};

pub(super) struct Parser<'a> {
    pub(super) lines: &'a [Line],
    pub(super) pos: usize,
    pub(super) opts: &'a DecoderOptions,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Line> {
        self.lines.get(self.pos)
    }
    fn next(&mut self) -> Option<&Line> {
        let l = self.lines.get(self.pos);
        self.pos += 1;
        l
    }

    pub(super) fn parse_root(&mut self) -> Result<Value, ToonError> {
        // §5: root form discovery.
        // A ROOT array is signalled by a keyless header: `[N]:` or `[N]{fields}:`.
        // A header WITH a key (e.g. `tags[3]: a,b`) is a field of a root object.
        let first = &self.lines[0];
        if let Some(h) = try_parse_header(&first.content) {
            if h.key.is_none() {
                return self.parse_array_body(0, None);
            }
        }
        // Single primitive: one non-empty line that is neither a header nor key:value.
        if self.lines.len() == 1
            && try_parse_header(&first.content).is_none()
            && parse_key_rest(&first.content).is_none()
        {
            let val = decode_primitive(&first.content, first.lineno)?;
            self.pos += 1;
            return Ok(val);
        }
        // Otherwise: root object.
        self.parse_object_at(0)
    }

    // Parse an object whose fields start at `depth`.
    fn parse_object_at(&mut self, depth: usize) -> Result<Value, ToonError> {
        let mut map = Map::new();
        while let Some(line) = self.peek() {
            if line.depth != depth {
                break;
            }
            // List items at this depth don't belong to this object.
            if line.content.starts_with("- ") || line.content == "-" {
                break;
            }
            let lineno = line.lineno;
            let content = line.content.clone();

            // Is it an array header with a key?
            if let Some(h) = try_parse_header(&content) {
                let key = h.key.clone().ok_or(ToonError::MissingColon(lineno))?;
                self.pos += 1;
                let val = self.parse_array_body(depth + 1, Some(&h.clone()))?;
                map.insert(key, val);
                continue;
            }

            // Must be key: rest
            let (key, rest) = parse_key_rest(&content).ok_or(ToonError::MissingColon(lineno))?;
            self.pos += 1;

            if rest.is_empty() {
                // Value is on subsequent lines.
                let val = if let Some(next) = self.peek() {
                    if next.depth == depth + 1 {
                        // Check if it's a nested array header or object.
                        if try_parse_header(&next.content).is_some() {
                            self.parse_array_body(depth + 1, None)?
                        } else if next.content.starts_with("- ") || next.content == "-" {
                            // list items without an enclosing array header — treat as expanded array
                            self.parse_list_items_at(depth + 1)?
                        } else {
                            self.parse_object_at(depth + 1)?
                        }
                    } else {
                        // Empty object.
                        Value::Object(Map::new())
                    }
                } else {
                    Value::Object(Map::new())
                };
                map.insert(key, val);
            } else {
                // Inline scalar value.
                let val = decode_primitive(rest.trim(), lineno)?;
                map.insert(key, val);
            }
        }
        Ok(Value::Object(map))
    }

    // Parse the body of an array (the header has already been consumed by caller).
    // `outer_header` is the parsed header, or None for root arrays that need re-parsing.
    fn parse_array_body(
        &mut self,
        depth: usize,
        outer: Option<&Header>,
    ) -> Result<Value, ToonError> {
        // Parse the header if we haven't yet (root array case).
        let header = if let Some(h) = outer {
            h.clone()
        } else {
            let line = self.next().ok_or(ToonError::Unexpected(0))?;
            let lineno = line.lineno;
            try_parse_header(&line.content).ok_or(ToonError::Unexpected(lineno))?
        };

        let count = header.count;

        // Empty array.
        if count == 0 {
            return Ok(Value::Array(vec![]));
        }

        // Inline primitive array: values are already in header.inline_rest
        if let Some(inline) = &header.inline_rest {
            if header.fields.is_none() {
                let vals = split_on_delim(inline, header.delim);
                if self.opts.strict && vals.len() != count {
                    return Err(ToonError::CountMismatch {
                        declared: count,
                        found: vals.len(),
                    });
                }
                let arr: Result<Vec<Value>, ToonError> = vals
                    .iter()
                    .enumerate()
                    .map(|(i, s)| decode_primitive(s.trim(), i + 1))
                    .collect();
                return Ok(Value::Array(arr?));
            }
        }

        // Tabular array.
        if let Some(fields) = &header.fields {
            let fields = fields.clone();
            let delim = header.delim;
            let mut rows: Vec<Value> = Vec::new();
            while let Some(line) = self.peek() {
                if line.depth != depth {
                    break;
                }
                let lineno = line.lineno;
                let content = line.content.clone();
                self.pos += 1;
                let cells = split_on_delim(&content, delim);
                if self.opts.strict && cells.len() != fields.len() {
                    return Err(ToonError::WidthMismatch {
                        expected: fields.len(),
                        found: cells.len(),
                        lineno,
                    });
                }
                let mut obj = Map::new();
                for (field, cell) in fields.iter().zip(cells.iter()) {
                    let key = decode_key(field);
                    let val = decode_primitive(cell.trim(), lineno)?;
                    obj.insert(key, val);
                }
                rows.push(Value::Object(obj));
            }
            if self.opts.strict && rows.len() != count {
                return Err(ToonError::CountMismatch {
                    declared: count,
                    found: rows.len(),
                });
            }
            return Ok(Value::Array(rows));
        }

        // Expanded array — items start with "- ".
        let items = self.parse_list_items_at(depth)?;
        if let Value::Array(ref arr) = items {
            if self.opts.strict && arr.len() != count {
                return Err(ToonError::CountMismatch {
                    declared: count,
                    found: arr.len(),
                });
            }
        }
        Ok(items)
    }

    // Parse list items (lines starting with "- ") at the given depth.
    fn parse_list_items_at(&mut self, depth: usize) -> Result<Value, ToonError> {
        let mut items: Vec<Value> = Vec::new();
        while let Some(l) = self.peek() {
            let (line_depth, line_content, lineno) = (l.depth, l.content.clone(), l.lineno);
            if line_depth != depth {
                break;
            }
            if !line_content.starts_with("- ") && line_content != "-" {
                break;
            }

            let rest_owned: String = if line_content == "-" {
                String::new()
            } else {
                line_content["- ".len()..].to_string()
            };
            let rest: &str = &rest_owned;
            self.pos += 1;

            if rest.is_empty() {
                // Empty object item.
                items.push(Value::Object(Map::new()));
                continue;
            }

            // Is the rest an array header?
            if let Some(h) = try_parse_header(rest) {
                if h.key.is_none() {
                    // Pure array item e.g. `- [3]: a,b,c`
                    let val = self.parse_array_body(depth + 1, Some(&h))?;
                    items.push(val);
                    continue;
                }
                // Keyed array as first field of an object item: e.g. `- tags[3]: a,b,c`
                let mut obj = Map::new();
                let key = h.key.clone().ok_or(ToonError::MissingColon(lineno))?;
                let first_arr = self.parse_array_body(depth + 2, Some(&h))?;
                obj.insert(key, first_arr);
                // Remaining fields at depth+1
                while let Some(next) = self.peek() {
                    if next.depth != depth + 1 {
                        break;
                    }
                    if next.content.starts_with("- ") || next.content == "-" {
                        break;
                    }
                    let nc = next.content.clone();
                    let nl = next.lineno;
                    if let Some(h2) = try_parse_header(&nc) {
                        let k2 = h2.key.clone().ok_or(ToonError::MissingColon(nl))?;
                        self.pos += 1;
                        let av = self.parse_array_body(depth + 2, Some(&h2))?;
                        obj.insert(k2, av);
                    } else if let Some((k, r)) = parse_key_rest(&nc) {
                        self.pos += 1;
                        if r.is_empty() {
                            let v = self.parse_value_at(depth + 2, nl)?;
                            obj.insert(k, v);
                        } else {
                            obj.insert(k, decode_primitive(r.trim(), nl)?);
                        }
                    } else {
                        break;
                    }
                }
                items.push(Value::Object(obj));
                continue;
            }

            // Is it a key-value pair (object item)?
            if let Some((k, r)) = parse_key_rest(rest) {
                let mut obj = Map::new();
                let first_val = if r.is_empty() {
                    // Value at depth+1
                    self.parse_value_at(depth + 1, lineno)?
                } else {
                    decode_primitive(r.trim(), lineno)?
                };
                obj.insert(k, first_val);
                // More fields of this object at depth+1
                while let Some(next) = self.peek() {
                    if next.depth != depth + 1 {
                        break;
                    }
                    if next.content.starts_with("- ") || next.content == "-" {
                        break;
                    }
                    let nc = next.content.clone();
                    let nl = next.lineno;
                    if let Some(h2) = try_parse_header(&nc) {
                        let k2 = h2.key.clone().ok_or(ToonError::MissingColon(nl))?;
                        self.pos += 1;
                        let av = self.parse_array_body(depth + 2, Some(&h2))?;
                        obj.insert(k2, av);
                    } else if let Some((k2, r2)) = parse_key_rest(&nc) {
                        self.pos += 1;
                        let v = if r2.is_empty() {
                            self.parse_value_at(depth + 2, nl)?
                        } else {
                            decode_primitive(r2.trim(), nl)?
                        };
                        obj.insert(k2, v);
                    } else {
                        break;
                    }
                }
                items.push(Value::Object(obj));
                continue;
            }

            // Pure primitive item.
            items.push(decode_primitive(rest, lineno)?);
        }
        Ok(Value::Array(items))
    }

    // Parse a value at the given depth (used when `key:` has no inline value).
    fn parse_value_at(&mut self, depth: usize, _ctx: usize) -> Result<Value, ToonError> {
        if let Some(line) = self.peek() {
            if line.depth == depth {
                let content = line.content.clone();
                if let Some(h) = try_parse_header(&content) {
                    return self.parse_array_body(depth + 1, Some(&h));
                }
                if content.starts_with("- ") || content == "-" {
                    return self.parse_list_items_at(depth);
                }
                return self.parse_object_at(depth);
            }
        }
        Ok(Value::Object(Map::new()))
    }
}
