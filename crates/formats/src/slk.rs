//! SYLK spreadsheets (`.slk`), which hold the game's object data tables.

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
}

impl Value {
    pub fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
            Value::Num(n) => format!("{n}"),
        }
    }
}

#[derive(Default, Debug)]
pub struct Table {
    /// Column headers from the first row.
    pub columns: Vec<String>,
    /// Rows keyed by the value of the first column.
    pub rows: HashMap<String, Vec<Option<Value>>>,
    /// Row keys in file order.
    pub order: Vec<String>,
}

impl Table {
    pub fn parse(text: &str) -> Table {
        let mut cells: HashMap<(usize, usize), Value> = HashMap::new();
        let (mut x, mut y) = (1usize, 1usize);
        let (mut max_x, mut max_y) = (0, 0);
        for line in text.lines() {
            let mut fields = line.split(';');
            let rtype = fields.next().unwrap_or("");
            if rtype != "C" && rtype != "F" {
                continue;
            }
            let mut val = None;
            for f in fields {
                let (k, v) = f.split_at(f.len().min(1));
                match k {
                    "X" => x = v.parse().unwrap_or(x),
                    "Y" => y = v.parse().unwrap_or(y),
                    "K" if rtype == "C" => {
                        val = Some(if let Some(s) = v.strip_prefix('"') {
                            Value::Str(s.strip_suffix('"').unwrap_or(s).to_string())
                        } else if let Ok(n) = v.parse::<f64>() {
                            Value::Num(n)
                        } else {
                            Value::Str(v.to_string())
                        });
                    }
                    _ => {}
                }
            }
            if let Some(v) = val {
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                cells.insert((x, y), v);
            }
        }
        let mut t = Table::default();
        t.columns = (1..=max_x)
            .map(|cx| cells.get(&(cx, 1)).map(Value::as_str).unwrap_or_default())
            .collect();
        for ry in 2..=max_y {
            let Some(key) = cells.get(&(1, ry)).map(Value::as_str) else { continue };
            let row = (1..=max_x).map(|cx| cells.get(&(cx, ry)).cloned()).collect();
            t.order.push(key.clone());
            t.rows.insert(key, row);
        }
        t
    }

    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.eq_ignore_ascii_case(name))
    }

    pub fn get(&self, row: &str, col: &str) -> Option<&Value> {
        let c = self.column(col)?;
        self.rows.get(row)?.get(c)?.as_ref()
    }
}
