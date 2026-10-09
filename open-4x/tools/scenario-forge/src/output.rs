//! Readable, diff-friendly JSON: terrain rows one per line, number lists wrapped, small
//! objects (a city, an army, a nation) on a single line.
use serde_json::Value;

const COMPACT: usize = 220;
const WRAP: usize = 118;

pub fn render(value: &Value) -> String {
    let mut out = String::new();
    write(value, 0, &mut out);
    out.push('\n');
    out
}

fn write(value: &Value, depth: usize, out: &mut String) {
    let compact = serde_json::to_string(value).expect("value serializes");
    if compact.len() <= COMPACT {
        out.push_str(&compact);
        return;
    }
    let pad = "  ".repeat(depth + 1);
    let close = "  ".repeat(depth);
    match value {
        Value::Array(items) if items.iter().all(Value::is_number) => {
            out.push_str("[\n");
            let mut line = pad.clone();
            for (i, item) in items.iter().enumerate() {
                let text = item.to_string();
                let separator = if i + 1 < items.len() { "," } else { "" };
                if line.len() + text.len() + 2 > WRAP && line.len() > pad.len() {
                    out.push_str(line.trim_end());
                    out.push('\n');
                    line = pad.clone();
                }
                line.push_str(&text);
                line.push_str(separator);
                line.push(' ');
            }
            out.push_str(line.trim_end());
            out.push('\n');
            out.push_str(&close);
            out.push(']');
        }
        Value::Array(items) => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&pad);
                write(item, depth + 1, out);
                out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&close);
            out.push(']');
        }
        Value::Object(entries) => {
            out.push_str("{\n");
            for (i, (key, item)) in entries.iter().enumerate() {
                out.push_str(&pad);
                out.push_str(&serde_json::to_string(key).expect("key serializes"));
                out.push_str(": ");
                write(item, depth + 1, out);
                out.push_str(if i + 1 < entries.len() { ",\n" } else { "\n" });
            }
            out.push_str(&close);
            out.push('}');
        }
        _ => out.push_str(&compact),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn small_values_stay_on_one_line_and_large_ones_wrap() {
        let small = json!({"id": "a", "n": [1, 2, 3]});
        assert_eq!(render(&small), "{\"id\":\"a\",\"n\":[1,2,3]}\n");
        let numbers: Vec<u32> = (0..200).collect();
        let text = render(&json!({ "runs": numbers, "rows": ["x".repeat(300), "y".repeat(300)] }));
        assert!(text.lines().all(|line| line.len() <= 330), "{text}");
        assert!(text.lines().count() > 8);
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap()["runs"][199],
            199
        );
    }
}
