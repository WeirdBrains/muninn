//! The Model Designer's text forms, shared by the CLI, the TUI and rules (see
//! `docs/models.md`):
//!
//! * fields: `name:type [required] [unique] [index] [default=V] [choices=a|b] [about="…"]`
//!   where the type is `text`, `int`, `real`, `bool`, `time`, `json` or `ref:MODEL` (a bare
//!   model name is a ref too) and `type!` is short for required. When changing a field the
//!   type may be left out, and `optional`, `-unique`, `-index`, `-default`, `-choices`
//!   take constraints away;
//! * field changes ([`FieldOp`]) and how a draft differs from the applied design;
//! * migration steps and plans as text;
//! * `FIELD=VALUE` assignments, `--where` conditions and `--sort` keys for records.
use crate::error::{Error, Result};
use crate::rules::{quote, split_list, tokenize};
use crate::store::models::{Design, Field, FieldOp, FieldType, Model, Plan, Step, coerce, new_field_id};
use crate::store::{Condition, Store};
use serde_json::{Map, Value};

/// A type as written: `text` … `ref:trades` (`model` is the model being designed, for
/// refs to itself).
pub fn parse_type(store: &Store, model: &Model, s: &str) -> Result<FieldType> {
    let s = s.trim();
    if let Some(t) = FieldType::basic(s) {
        return Ok(t);
    }
    let target = s.strip_prefix("ref:").unwrap_or(s);
    if target.is_empty() {
        return Err(Error::invalid("field type", "ref: needs a model, e.g. ref:trades"));
    }
    if target.eq_ignore_ascii_case(&model.name) || target == model.id {
        return Ok(FieldType::Ref(model.id.clone()));
    }
    match store.find_model(target) {
        Ok(m) => Ok(FieldType::Ref(m.id)),
        Err(_) => Err(Error::invalid("field type", format!("`{s}` is not a type ({}) or a model", FieldType::NAMES.join(", ")))),
    }
}

/// A type for people: `real`, `ref:trades`.
pub fn type_text(store: &Store, t: &FieldType) -> String {
    match t {
        FieldType::Ref(id) => format!("ref:{}", store.model(id).ok().flatten().map(|m| m.name).unwrap_or_else(|| id.clone())),
        t => t.code(),
    }
}

/// A type as stored in a step (`ref:<id>`) for people.
pub fn code_text(store: &Store, code: &str) -> String {
    match FieldType::try_from(code.to_string()) {
        Ok(t) => type_text(store, &t),
        Err(_) => code.to_string(),
    }
}

/// Parse a field spec into `base` (a new field when `None`; with `Some`, a change that
/// keeps what it does not mention).
pub fn parse_field(store: &Store, model: &Model, spec: &str, base: Option<&Field>) -> Result<Field> {
    let toks = tokenize(spec)?;
    let Some(((head, hv), rest)) = toks.split_first() else {
        return Err(Error::invalid("field", "is empty: write NAME:TYPE, e.g. price:real"));
    };
    if hv.is_some() {
        return Err(Error::invalid("field", format!("`{spec}`: start with NAME:TYPE, e.g. price:real")));
    }
    let (name, ty) = match head.split_once(':') {
        Some((n, t)) => (n.trim().to_string(), Some(t.trim())),
        None => (head.trim().to_string(), None),
    };
    let mut f = match base {
        Some(b) => b.clone(),
        None => {
            let Some(t) = ty else {
                return Err(Error::invalid("field", format!("`{name}` needs a type: {name}:text, {name}:real, … ({})", FieldType::NAMES.join(", "))));
            };
            let _ = t;
            Field::new(&name, FieldType::Text)
        }
    };
    f.name = name;
    if let Some(t) = ty {
        let (t, req) = match t.strip_suffix('!') {
            Some(t) => (t, true),
            None => (t, false),
        };
        f.ty = parse_type(store, model, t)?;
        if req {
            f.required = true;
        }
    }
    let mut default: Option<Option<String>> = None;
    for (k, v) in rest {
        match (k.to_ascii_lowercase().as_str(), v) {
            ("required" | "req", None) => f.required = true,
            ("optional" | "-required", None) => f.required = false,
            ("unique", None) => f.unique = true,
            ("-unique", None) => f.unique = false,
            ("index" | "indexed", None) => f.index = true,
            ("-index", None) => f.index = false,
            ("-default", None) => default = Some(None),
            ("-choices", None) => f.choices.clear(),
            ("-about", None) => f.about.clear(),
            ("default", Some(v)) => default = Some(Some(v.clone())),
            ("choices", Some(v)) => f.choices = v.split(['|', ',']).map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect(),
            ("about", Some(v)) => f.about = v.trim().to_string(),
            (other, _) => {
                return Err(Error::invalid(
                    "field",
                    format!(
                        "`{other}`: use required, optional, unique, index, default=V, choices=a|b, about=\"…\" (or -unique, -index, -default, -choices)"
                    ),
                ));
            }
        }
    }
    if !f.choices.is_empty() && f.ty != FieldType::Text {
        return Err(Error::invalid("field", format!("{}: choices are for text fields", f.name)));
    }
    match default {
        Some(None) => f.default = None,
        Some(Some(v)) => {
            let mut probe = f.clone();
            probe.required = false;
            let d = coerce(&probe, Value::String(v.clone()))?;
            f.default = (!d.is_null()).then_some(d);
        }
        None => {
            if let Some(d) = f.default.clone() {
                // a type change: the old default must still fit
                let mut probe = f.clone();
                probe.required = false;
                f.default = coerce(&probe, d).ok().filter(|v| !v.is_null());
            }
        }
    }
    Ok(f)
}

/// A field as a spec (parses back to the same field, ids aside).
pub fn field_text(store: &Store, f: &Field) -> String {
    let mut s = format!("{}:{}", f.name, type_text(store, &f.ty));
    if f.required {
        s.push_str(" required");
    }
    if f.unique {
        s.push_str(" unique");
    }
    if f.index {
        s.push_str(" index");
    }
    if let Some(d) = &f.default {
        s.push_str(&format!(" default={}", quote(&value_text(d))));
    }
    if !f.choices.is_empty() {
        s.push_str(&format!(" choices={}", quote(&f.choices.join("|"))));
    }
    if !f.about.is_empty() {
        s.push_str(&format!(" about={}", quote(&f.about)));
    }
    s
}

/// `;`-separated field specs (as `model create --fields` takes them).
pub fn parse_fields(store: &Store, model: &Model, s: &str) -> Result<Vec<Field>> {
    split_list(s).iter().map(|f| parse_field(store, model, f, None)).collect()
}

/// A value for people (strings without quotes).
pub fn value_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        o => o.to_string(),
    }
}

/// Apply field changes to a design, in order.
pub fn apply_ops(store: &Store, model: &Model, design: &Design, ops: &[FieldOp]) -> Result<Design> {
    let mut d = design.clone();
    let find = |d: &Design, n: &str| -> Result<usize> {
        d.fields.iter().position(|f| f.name.eq_ignore_ascii_case(n.trim())).ok_or_else(|| {
            let names: Vec<&str> = d.fields.iter().map(|f| f.name.as_str()).collect();
            Error::invalid(
                "field",
                format!(
                    "{} has no field `{}` (fields: {})",
                    model.name,
                    n.trim(),
                    if names.is_empty() { "none yet".into() } else { names.join(", ") }
                ),
            )
        })
    };
    for op in ops {
        match op {
            FieldOp::Add { spec } => {
                let mut f = parse_field(store, model, spec, None)?;
                if d.field(&f.name).is_some() {
                    return Err(Error::invalid("field", format!("{} already has `{}` (change it instead)", model.name, f.name)));
                }
                f.id = new_field_id();
                d.fields.push(f);
            }
            FieldOp::Change { spec } => {
                let name = spec.split(|c: char| c == ':' || c.is_whitespace()).next().unwrap_or_default();
                let i = find(&d, name)?;
                let f = parse_field(store, model, spec, Some(&d.fields[i]))?;
                d.fields[i] = f;
            }
            FieldOp::Drop { field } => {
                let i = find(&d, field)?;
                d.fields.remove(i);
            }
            FieldOp::Rename { from, to } => {
                let i = find(&d, from)?;
                let to = to.trim();
                if d.fields.iter().enumerate().any(|(j, f)| j != i && f.name == to) {
                    return Err(Error::invalid("field", format!("{} already has `{to}`", model.name)));
                }
                d.fields[i].name = to.to_string();
            }
            FieldOp::Move { field, to } => {
                let i = find(&d, field)?;
                let f = d.fields.remove(i);
                let at = to.saturating_sub(1).min(d.fields.len());
                d.fields.insert(at, f);
            }
        }
    }
    store.check_design(model, &d)?;
    Ok(d)
}

/// Parse `NAME:TYPE…` (add), `rename a:b`, `move f:2` style CLI values into ops.
pub fn parse_rename(s: &str) -> Result<FieldOp> {
    let (from, to) = s.split_once([':', '=']).ok_or_else(|| Error::invalid("rename", format!("`{s}`: write OLD:NEW")))?;
    Ok(FieldOp::Rename { from: from.trim().to_string(), to: to.trim().to_string() })
}

pub fn parse_move(s: &str) -> Result<FieldOp> {
    let (f, to) = s.split_once([':', '=']).ok_or_else(|| Error::invalid("move", format!("`{s}`: write FIELD:POSITION (1 is first)")))?;
    let to: usize = to.trim().parse().map_err(|_| Error::invalid("move", format!("`{s}`: the position is a number (1 is first)")))?;
    Ok(FieldOp::Move { field: f.trim().to_string(), to })
}

/// One op as text, e.g. `add price:real`, `rename a → b`.
pub fn op_text(op: &FieldOp) -> String {
    match op {
        FieldOp::Add { spec } => format!("add {spec}"),
        FieldOp::Change { spec } => format!("change {spec}"),
        FieldOp::Drop { field } => format!("drop {field}"),
        FieldOp::Rename { from, to } => format!("rename {from} → {to}"),
        FieldOp::Move { field, to } => format!("move {field} to {to}"),
    }
}

/// One op as `key=value` of the `model-edit` then-action.
pub fn op_arg(op: &FieldOp) -> String {
    match op {
        FieldOp::Add { spec } => format!("add={}", quote(spec)),
        FieldOp::Change { spec } => format!("change={}", quote(spec)),
        FieldOp::Drop { field } => format!("drop={}", quote(field)),
        FieldOp::Rename { from, to } => format!("rename={}", quote(&format!("{from}:{to}"))),
        FieldOp::Move { field, to } => format!("move={}", quote(&format!("{field}:{to}"))),
    }
}

/// The ops that turn `old` into `new` (fields are matched by id, so renames show).
pub fn diff_ops(store: &Store, old: &Design, new: &Design) -> Vec<FieldOp> {
    let mut ops = vec![];
    for f in &old.fields {
        if new.by_id(&f.id).is_none() {
            ops.push(FieldOp::Drop { field: f.name.clone() });
        }
    }
    for f in &new.fields {
        let Some(o) = old.by_id(&f.id) else {
            ops.push(FieldOp::Add { spec: field_text(store, f) });
            continue;
        };
        if o.name != f.name {
            ops.push(FieldOp::Rename { from: o.name.clone(), to: f.name.clone() });
        }
        if (Field { name: f.name.clone(), ..o.clone() }) != *f {
            let mut s = field_text(store, f);
            for (had, has, off) in [
                (o.required, f.required, " optional"),
                (o.unique, f.unique, " -unique"),
                (o.index, f.index, " -index"),
                (o.default.is_some(), f.default.is_some(), " -default"),
                (!o.choices.is_empty(), !f.choices.is_empty(), " -choices"),
                (!o.about.is_empty(), !f.about.is_empty(), " -about"),
            ] {
                if had && !has {
                    s.push_str(off);
                }
            }
            ops.push(FieldOp::Change { spec: s });
        }
    }
    // order: after adds/drops, are the kept fields in a different order?
    let mut now: Vec<String> = old.fields.iter().filter(|f| new.by_id(&f.id).is_some()).map(|f| f.id.clone()).collect();
    now.extend(new.fields.iter().filter(|f| old.by_id(&f.id).is_none()).map(|f| f.id.clone()));
    for (i, f) in new.fields.iter().enumerate() {
        if now.get(i) != Some(&f.id) {
            now.retain(|x| x != &f.id);
            now.insert(i, f.id.clone());
            ops.push(FieldOp::Move { field: f.name.clone(), to: i + 1 });
        }
    }
    ops
}

/// `+` adds, `-` loses, `~` changes, `·` the rest (the TUI colors by this).
pub fn step_mark(s: &Step) -> char {
    match s {
        Step::CreateTable { .. } | Step::AddField { .. } | Step::ShowField { .. } => '+',
        Step::DropField { .. } | Step::HideField { .. } => '-',
        Step::ChangeType { lost, .. } if *lost > 0 => '-',
        Step::ChangeType { .. } | Step::RenameField { .. } => '~',
        _ => '·',
    }
}

fn n(k: usize, one: &str) -> String {
    format!("{k} {one}{}", if k == 1 { "" } else { "s" })
}

/// One step for people.
pub fn step_text(store: &Store, s: &Step) -> String {
    let with = |filled: &usize, w: &Option<Value>| match w {
        Some(v) if *filled > 0 => format!(", filling {} with {}", n(*filled, "record"), quote(&value_text(v))),
        _ => String::new(),
    };
    match s {
        Step::CreateTable { table } => format!("create table {table}"),
        Step::AddField { field, ty, filled, with: w } => format!("add {field} ({}){}", code_text(store, ty), with(filled, w)),
        Step::DropField { field, ty, lost } => {
            format!("drop {field} ({}){}", code_text(store, ty), if *lost > 0 { format!(", losing {}", n(*lost, "value")) } else { String::new() })
        }
        Step::RenameField { from, to } => format!("rename {from} → {to}"),
        Step::ChangeType { field, from, to, lost } => format!(
            "change {field} {} → {}{}",
            code_text(store, from),
            code_text(store, to),
            if *lost > 0 { format!(", {} do not fit and are cleared", n(*lost, "value")) } else { String::new() }
        ),
        Step::Require { field, on: true, filled, with: w } => format!("make {field} required{}", with(filled, w)),
        Step::Require { field, on: false, .. } => format!("make {field} optional"),
        Step::Unique { field, on } => format!("{} {field} unique", if *on { "make" } else { "stop making" }),
        Step::Index { field, on } => format!("{} index on {field}", if *on { "add" } else { "drop" }),
        Step::Default { field, value: Some(v) } => format!("default {field} to {}", quote(&value_text(v))),
        Step::Default { field, value: None } => format!("remove the default of {field}"),
        Step::Choices { field, choices } if choices.is_empty() => format!("allow any value in {field}"),
        Step::Choices { field, choices } => format!("limit {field} to {}", choices.join(" | ")),
        Step::About { field } => format!("describe {field}"),
        Step::Order { fields } => format!("order fields: {}", fields.join(", ")),
        Step::HideField { field } => format!("drop {field} from the model (the bars keep their imported values; rolling back shows them again)"),
        Step::ShowField { field } => format!("show {field} (the bars' imported values)"),
    }
}

/// A plan as lines: a header, `+ add …` steps, then `✗ …` for what blocks it.
pub fn plan_lines(store: &Store, p: &Plan) -> Vec<String> {
    let mut out = vec![format!("{}: v{} → v{} ({})", p.model, p.from, p.to, n(p.rows, "record"))];
    if p.steps.is_empty() {
        out.push("  no changes".into());
    }
    out.extend(p.steps.iter().map(|s| format!("  {} {}", step_mark(s), step_text(store, s))));
    out.extend(p.blocked.iter().map(|b| format!("  ✗ {b}")));
    if p.blocked.is_empty() && p.lost() > 0 {
        out.push(format!("  ! loses {}: confirm with -y/--yes", n(p.lost(), "value")));
    }
    out
}

/// `FIELD=VALUE …` (an empty value clears a field).
pub fn parse_assignments(items: &[String]) -> Result<Map<String, Value>> {
    let mut m = Map::new();
    for it in items {
        let (k, v) = it.split_once('=').ok_or_else(|| Error::invalid("value", format!("`{it}`: write FIELD=VALUE")))?;
        let k = k.trim();
        if k.is_empty() {
            return Err(Error::invalid("value", format!("`{it}`: write FIELD=VALUE")));
        }
        m.insert(k.to_string(), if v.is_empty() { Value::Null } else { Value::String(v.to_string()) });
    }
    Ok(m)
}

/// `--where` values (each may hold several `;`-separated conditions).
pub fn parse_where(items: &[String]) -> Result<Vec<Condition>> {
    items
        .iter()
        .flat_map(|s| split_list(s))
        .map(|c| crate::rules::parse_condition(&c).map_err(|e| Error::invalid("--where", e.to_string())))
        .collect()
}

/// `--sort price`, `--sort -price`, `--sort price:desc` (comma-separated or repeated).
pub fn parse_sort(items: &[String]) -> Vec<(String, bool)> {
    items
        .iter()
        .flat_map(|s| s.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| match (s.strip_prefix('-'), s.rsplit_once(':')) {
            (Some(f), _) => (f.to_string(), true),
            (None, Some((f, d))) => (f.to_string(), d.eq_ignore_ascii_case("desc")),
            _ => (s.to_string(), false),
        })
        .collect()
}

/// An action to record in the history (front ends add their origin).
#[derive(Debug, Clone, PartialEq)]
pub struct Act {
    pub kind: &'static str,
    pub summary: String,
    pub params: Value,
}

fn model_json(m: &Model) -> Value {
    serde_json::json!({ "model": m.id, "name": m.name, "version": m.version })
}

pub fn act_model_create(m: &Model) -> Act {
    Act { kind: "model.create", summary: format!("create model {}", m.name), params: model_json(m) }
}

/// Field changes saved to the draft (`renamed`: the old name, when the model was renamed).
pub fn act_model_edit(m: &Model, ops: &[FieldOp], renamed: Option<&str>) -> Act {
    let mut p = model_json(m);
    p["ops"] = ops.iter().map(op_text).collect::<Vec<_>>().into();
    p["draft"] = m.draft.is_some().into();
    if let Some(o) = renamed {
        p["from"] = o.into();
    }
    let what = match (renamed, ops.is_empty()) {
        (Some(o), _) => format!("rename model {o} to {}", m.name),
        (None, true) => format!("edit model {}", m.name),
        (None, false) => format!("edit model {}: {}", m.name, ops.iter().map(op_text).collect::<Vec<_>>().join(", ")),
    };
    Act { kind: "model.edit", summary: what, params: p }
}

pub fn act_model_discard(m: &Model) -> Act {
    Act { kind: "model.edit", summary: format!("discard the draft of model {}", m.name), params: model_json(m) }
}

/// A migration (`rollback`: it went back to an earlier design).
pub fn act_model_apply(store: &Store, m: &Model, p: &Plan, rollback: bool) -> Act {
    let mut j = model_json(m);
    j["from"] = p.from.into();
    j["to"] = p.to.into();
    j["steps"] = p.steps.iter().map(|s| step_text(store, s)).collect::<Vec<_>>().into();
    j["lost"] = p.lost().into();
    j["records"] = p.rows.into();
    let (kind, verb) = if rollback { ("model.rollback", "roll back") } else { ("model.apply", "apply") };
    Act { kind, summary: format!("{verb} model {} v{} → v{} ({})", m.name, p.from, p.to, n(p.steps.len(), "step")), params: j }
}

pub fn act_model_remove(m: &Model, records: usize) -> Act {
    let mut p = model_json(m);
    p["records"] = records.into();
    Act { kind: "model.remove", summary: format!("remove model {} ({})", m.name, n(records, "record")), params: p }
}

pub fn act_record_create(m: &Model, r: &crate::store::Record) -> Act {
    Act {
        kind: "record.create",
        summary: format!("create {} record {}", m.name, r.short_id()),
        params: serde_json::json!({ "model": m.id, "name": m.name, "record": r.id, "values": r.values }),
    }
}

/// `one`: the record when one was changed by id; else `count` matched `filter`.
pub fn act_record_update(m: &Model, one: Option<&crate::store::Record>, count: usize, filter: &[Condition], changes: &Map<String, Value>) -> Act {
    let w: Vec<String> = filter.iter().map(crate::rules::condition_text).collect();
    let summary = match one {
        Some(r) => format!("update {} record {}", m.name, r.short_id()),
        None => format!("update {} {} record{}", count, m.name, if count == 1 { "" } else { "s" }),
    };
    Act {
        kind: "record.update",
        summary,
        params: serde_json::json!({ "model": m.id, "name": m.name, "record": one.map(|r| r.id.clone()), "count": count, "where": w.join("; "), "values": changes }),
    }
}

pub fn act_record_delete(m: &Model, one: Option<&crate::store::Record>, count: usize, filter: &[Condition]) -> Act {
    let w: Vec<String> = filter.iter().map(crate::rules::condition_text).collect();
    let summary = match one {
        Some(r) => format!("delete {} record {}", m.name, r.short_id()),
        None => format!("delete {} {} record{}", count, m.name, if count == 1 { "" } else { "s" }),
    };
    Act {
        kind: "record.delete",
        summary,
        params: serde_json::json!({ "model": m.id, "name": m.name, "record": one.map(|r| r.id.clone()), "count": count, "where": w.join("; "), "values": one.map(|r| r.values.clone()) }),
    }
}

/// Where a draft stands: `v2 applied · draft: 3 changes` or `v2 applied`.
pub fn status_text(store: &Store, m: &Model) -> String {
    let base = if m.version == 0 { "not applied yet".to_string() } else { format!("v{} applied", m.version) };
    match &m.draft {
        Some(d) => {
            let k = diff_ops(store, &m.design, d).len();
            format!("{base} · draft: {}", n(k, "change"))
        }
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fields_parse_print_and_change() {
        let s = Store::open_in_memory().unwrap();
        let (m, _) = s.create_model("trades", "", Design::default(), false).unwrap();
        let f = parse_field(&s, &m, "side:text! default=buy choices=buy|sell about=\"which way\"", None).unwrap();
        assert!(f.required && f.default == Some(json!("buy")) && f.choices == ["buy", "sell"] && f.about == "which way");
        assert_eq!(field_text(&s, &f), "side:text required default=buy choices=buy|sell about=\"which way\"");
        let back = parse_field(&s, &m, &field_text(&s, &f), None).unwrap();
        assert_eq!(Field { id: f.id.clone(), ..back }, f);
        assert!(parse_field(&s, &m, "price", None).unwrap_err().to_string().contains("needs a type"));
        assert!(parse_field(&s, &m, "price:money", None).unwrap_err().to_string().contains("not a type"));
        assert!(parse_field(&s, &m, "n:int choices=a", None).is_err());
        assert!(parse_field(&s, &m, "n:int default=x", None).is_err());
        assert_eq!(parse_field(&s, &m, "parent:trades", None).unwrap().ty, FieldType::Ref(m.id.clone()));
        assert_eq!(type_text(&s, &parse_field(&s, &m, "bar:ref:bars", None).unwrap().ty), "ref:bars");

        let d = apply_ops(
            &s,
            &m,
            &Design::default(),
            &[FieldOp::Add { spec: "price:real".into() }, FieldOp::Add { spec: "side:text default=buy".into() }],
        )
        .unwrap();
        let d2 = apply_ops(
            &s,
            &m,
            &d,
            &[
                FieldOp::Change { spec: "price required default=0".into() },
                FieldOp::Rename { from: "side".into(), to: "direction".into() },
                FieldOp::Move { field: "direction".into(), to: 1 },
                FieldOp::Add { spec: "note:text".into() },
            ],
        )
        .unwrap();
        assert_eq!(d2.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["direction", "price", "note"]);
        assert!(d2.fields[1].required && d2.fields[1].ty == FieldType::Real && d2.fields[1].id == d.fields[0].id);
        assert!(apply_ops(&s, &m, &d, &[FieldOp::Drop { field: "nope".into() }]).unwrap_err().to_string().contains("no field `nope`"));
        assert!(apply_ops(&s, &m, &d, &[FieldOp::Add { spec: "price:int".into() }]).is_err());
        let d3 = apply_ops(&s, &m, &d2, &[FieldOp::Change { spec: "price:int optional -default".into() }]).unwrap();
        assert!(!d3.fields[1].required && d3.fields[1].default.is_none() && d3.fields[1].ty == FieldType::Integer);

        // the diff replays to the same design
        let ops = diff_ops(&s, &d, &d2);
        let replay = apply_ops(&s, &m, &d, &ops).unwrap();
        let names = |d: &Design| d.fields.iter().map(|f| field_text(&s, f)).collect::<Vec<_>>();
        assert_eq!(names(&replay), names(&d2), "{ops:?}");
        let back = diff_ops(&s, &d2, &d3);
        assert_eq!(names(&apply_ops(&s, &m, &d2, &back).unwrap()), names(&d3));
        assert!(op_arg(&ops[0]).contains('='));
    }

    #[test]
    fn plans_read_well_and_values_parse() {
        let s = Store::open_in_memory().unwrap();
        let d = Design { fields: vec![Field::new("price", FieldType::Real)], ..Default::default() };
        let (m, _) = s.create_model("signals", "", d, true).unwrap();
        let m = s.edit_model(&m.id, &[FieldOp::Drop { field: "price".into() }, FieldOp::Add { spec: "note:text default=hi".into() }]).unwrap();
        let p = s.plan_model(&m, m.pending()).unwrap();
        let lines = plan_lines(&s, &p);
        assert_eq!(lines[0], "signals: v1 → v2 (0 records)");
        assert!(lines.iter().any(|l| l == "  - drop price (real)"), "{lines:?}");
        assert!(lines.iter().any(|l| l == "  + add note (text)"), "{lines:?}");
        let a = parse_assignments(&["price=1.5".into(), "note=".into()]).unwrap();
        assert_eq!((a["price"].clone(), a["note"].clone()), (json!("1.5"), Value::Null));
        assert!(parse_assignments(&["x".into()]).is_err());
        assert_eq!(parse_where(&["price > 1; side = buy".into()]).unwrap().len(), 2);
        assert_eq!(parse_sort(&["-price,side:desc".into(), "at".into()]), vec![("price".into(), true), ("side".into(), true), ("at".into(), false)]);
    }
}
