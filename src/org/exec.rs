//! Executables (2.13): the code spaces an org node serves, described in protobuf
//! (`muninn.code.v1.Executable`) and callable over gRPC, JSON and the web. Each served space
//! gets generated message and service definitions from its declared input (the record query a
//! caller may narrow) and its output (a [`RunResult`] whose view the source declares with
//! `@view`); all of it is generated here, in Rust, with no protoc.
use super::{Actor, can, require};
use crate::error::{Error, Result};
use crate::store::{CodeScope, CodeSpace, Org, Store};
use serde::Serialize;
use serde_json::{Value, json};

pub const PACKAGE: &str = "muninn.code.v1";
pub const COMMON_PROTO: &str = "muninn/code/v1/executable.proto";
pub const LIST_SERVICE: &str = "muninn.code.v1.Executables";

/// One input field (its number is fixed per name, so every space's input decodes the same).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Field {
    pub name: &'static str,
    pub number: u32,
    /// `string` or `int64`.
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub repeated: bool,
    pub doc: &'static str,
}

const QUERY_FIELDS: &[Field] = &[
    Field { name: "where", number: 1, kind: "string", repeated: true, doc: "more conditions (ANDed with the space's own)" },
    Field { name: "text", number: 2, kind: "string", repeated: false, doc: "full-text match (replaces the space's)" },
    Field { name: "sort", number: 3, kind: "string", repeated: true, doc: "sort fields, -f descending (replaces)" },
    Field { name: "group", number: 4, kind: "string", repeated: true, doc: "group fields (replaces)" },
    Field { name: "limit", number: 5, kind: "int64", repeated: false, doc: "at most this many records (never above the space's)" },
];

/// A served code space, as callers see it.
#[derive(Debug, Clone, Serialize)]
pub struct Executable {
    pub id: String,
    pub name: String,
    /// The generated service (`muninn.code.v1.o.<org>.<Service>`), with one `Run` method.
    pub service: String,
    pub language: String,
    /// The trust hash of the source: it changes with every new version.
    pub version_hash: String,
    pub scope: CodeScope,
    pub inputs: Vec<Field>,
    /// The declared view: table, chart, text or any.
    pub output: String,
    /// Where its `.proto` is: `/o/<org>/proto/<file>`.
    pub proto: String,
}

/// The fields a space takes: the record query when it reads a model, else none.
pub fn inputs(c: &CodeSpace) -> Vec<Field> {
    if c.input.is_empty() { vec![] } else { QUERY_FIELDS.to_vec() }
}

/// The view a source declares (`@view table|chart|value`).
pub fn output_kind(c: &CodeSpace) -> &'static str {
    for l in c.source.lines() {
        let l = l.trim().trim_start_matches(['#', '/']).trim();
        if let Some(v) = l.strip_prefix("@view") {
            return match v.trim() {
                "table" => "table",
                "chart" => "chart",
                "value" | "text" => "text",
                _ => "any",
            };
        }
    }
    "any"
}

/// A protobuf identifier from a name: `sma cross` → `SmaCross`.
pub fn pascal(name: &str) -> String {
    let mut out = String::new();
    for w in name.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()) {
        let mut cs = w.chars();
        if let Some(f) = cs.next() {
            out.push(f.to_ascii_uppercase());
            out.extend(cs);
        }
    }
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, 'X');
    }
    out
}

/// The org's protobuf package: `muninn.code.v1.o.comet_lab`.
pub fn package(org: &Org) -> String {
    let mut p: String = org.slug.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    if p.is_empty() || p.starts_with(|c: char| c.is_ascii_digit()) {
        p.insert(0, 'o');
    }
    format!("{PACKAGE}.o.{p}")
}

fn served(s: &Store, org: &Org) -> Result<Vec<(CodeSpace, String)>> {
    let mut seen: Vec<String> = vec![];
    let mut out = vec![];
    for id in s.served_code(&org.id)? {
        let Some(c) = s.code_space(&id)? else { continue };
        let mut svc = pascal(&c.name);
        if seen.contains(&svc) {
            svc = format!("{svc}{}", crate::store::short(&c.id).to_ascii_uppercase());
        }
        seen.push(svc.clone());
        out.push((c, svc));
    }
    Ok(out)
}

fn describe(s: &Store, org: &Org, c: &CodeSpace, svc: &str) -> Executable {
    Executable {
        id: c.id.clone(),
        name: c.name.clone(),
        service: format!("{}.{svc}", package(org)),
        language: c.lang.as_str().into(),
        version_hash: crate::code::trust_hash(c).unwrap_or_default(),
        scope: s.code_scope(&c.id).unwrap_or(CodeScope::User),
        inputs: inputs(c),
        output: output_kind(c).into(),
        proto: format!("/o/{}/proto/{svc}.proto", org.slug),
    }
}

/// What this node serves for the org that the caller may run.
pub fn executables(s: &Store, org: &Org, a: &Actor) -> Result<Vec<Executable>> {
    if !can(s, org, a, "code.run") {
        return Ok(vec![]);
    }
    Ok(served(s, org)?.iter().map(|(c, svc)| describe(s, org, c, svc)).collect())
}

/// A served space by id, name or generated service name (full or short).
pub fn find(s: &Store, org: &Org, key: &str) -> Result<(CodeSpace, Executable)> {
    let k = key.trim();
    for (c, svc) in served(s, org)? {
        let e = describe(s, org, &c, &svc);
        if c.id == k || c.name.eq_ignore_ascii_case(k) || svc == k || e.service == k {
            return Ok((c, e));
        }
    }
    Err(Error::invalid("executable", format!("`{k}` is not served by this node for {}", org.name)))
}

/// The caller's narrowing of a space's input.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Input {
    #[prost(string, repeated, tag = "1")]
    pub r#where: Vec<String>,
    #[prost(string, tag = "2")]
    pub text: String,
    #[prost(string, repeated, tag = "3")]
    pub sort: Vec<String>,
    #[prost(string, repeated, tag = "4")]
    pub group: Vec<String>,
    #[prost(int64, tag = "5")]
    pub limit: i64,
}

impl Input {
    pub fn from_json(v: &Value) -> Result<Input> {
        let strs = |k: &str| -> Result<Vec<String>> {
            match v.get(k) {
                None | Some(Value::Null) => Ok(vec![]),
                Some(Value::String(x)) => Ok(vec![x.clone()]),
                Some(Value::Array(a)) => {
                    a.iter().map(|x| x.as_str().map(str::to_string).ok_or_else(|| Error::invalid("input", format!("`{k}`: strings")))).collect()
                }
                _ => Err(Error::invalid("input", format!("`{k}`: a string or a list of strings"))),
            }
        };
        if let Some(o) = v.as_object()
            && let Some(k) = o.keys().find(|k| !QUERY_FIELDS.iter().any(|f| f.name == k.as_str()))
        {
            return Err(Error::invalid("input", format!("unknown field `{k}`: where, text, sort, group or limit")));
        }
        Ok(Input {
            r#where: strs("where")?,
            text: v.get("text").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
            sort: strs("sort")?,
            group: strs("group")?,
            limit: v.get("limit").and_then(|x| x.as_i64()).unwrap_or(0),
        })
    }

    fn is_empty(&self) -> bool {
        *self == Input::default()
    }

    /// The space's input narrowed by this one.
    fn apply(&self, c: &mut CodeSpace) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }
        if c.input.is_empty() {
            return Err(Error::invalid("input", format!("{} takes no input", c.name)));
        }
        c.input.filter.extend(self.r#where.iter().cloned());
        if !self.text.is_empty() {
            c.input.text = self.text.clone();
        }
        if !self.sort.is_empty() {
            c.input.sort = self.sort.clone();
        }
        if !self.group.is_empty() {
            c.input.group = self.group.clone();
        }
        if self.limit < 0 {
            return Err(Error::invalid("input", "limit: 0 or more"));
        }
        if self.limit > 0 {
            let l = self.limit as usize;
            c.input.limit = Some(c.input.limit.map_or(l, |m| m.min(l)));
        }
        Ok(())
    }
}

/// Run an executable for a caller (needs `code.run`; the node must serve it and its current
/// source must be approved here).
pub fn run(s: &Store, origin: &str, a: &Actor, org: &Org, key: &str, input: &Input) -> Result<Value> {
    require(s, org, a, "code.run")?;
    let (space, e) = find(s, org, key)?;
    if !crate::code::approved(&space) {
        return Err(Error::invalid("executable", format!("{} changed since it was approved; approve it on this node first", space.name)));
    }
    let mut run_as = space.clone();
    input.apply(&mut run_as)?;
    let env = crate::code::CodeEnv::for_store(s)?;
    let info = crate::code::run(s, &env, origin, &run_as, &mut |_| {})?;
    s.note_action(&crate::store::NewAction::new("org.run", origin, format!("{} ran {} in {}", a.who(), space.name, org.name)).params(json!({
        "org": org.id,
        "code": space.id,
        "service": e.service,
        "by": a.who()
    })));
    Ok(crate::code::run_json(&info))
}

// ---------- .proto text ----------

/// The shared definitions every executable uses.
pub fn common_proto() -> String {
    format!(
        r#"// Generated by Muninn {ver}. Executables an org node serves, and what they return.
syntax = "proto3";

package {PACKAGE};

// One input field of an executable.
message Field {{
  string name = 1;
  uint32 number = 2;
  string type = 3;    // string or int64
  bool repeated = 4;
  string doc = 5;
}}

message Scope {{
  string kind = 1;    // global, user or organization
  string org_id = 2;
}}

message Executable {{
  string id = 1;
  string name = 2;
  string service = 3;      // the generated service; call its Run method
  string language = 4;
  string version_hash = 5; // changes with every new source
  Scope scope = 6;
  repeated Field inputs = 7;
  string output = 8;       // the declared view: table, chart, text or any
  string proto = 9;        // where the service's .proto is
}}

message ListRequest {{
  string org = 1;          // the org's slug or id
}}

message ExecutableList {{
  repeated Executable executables = 1;
}}

message Row {{
  repeated string cells = 1;
}}

message Series {{
  string name = 1;
  repeated double values = 2; // NaN where there is no value
}}

// A run's view: a table, a chart, a text or tiles of views.
message View {{
  string kind = 1;         // table, chart, text or tiles
  string title = 2;
  repeated string columns = 3;
  repeated Row rows = 4;
  Series x = 5;
  bool x_time = 6;         // x holds epoch milliseconds
  repeated Series series = 7;
  string text = 8;
  repeated View tiles = 9;
  string dir = 10;         // right or below
}}

message RunResult {{
  string status = 1;       // ok or error
  string error = 2;
  View view = 3;
  repeated string logs = 4;
  int64 writes = 5;
  int64 ms = 6;
  string json = 7;         // the whole result as JSON
}}

service Executables {{
  // What the node serves for the org that you may run.
  rpc List(ListRequest) returns (ExecutableList);
}}
"#,
        ver = env!("CARGO_PKG_VERSION")
    )
}

/// One executable's `.proto`: its input message and service.
pub fn space_proto(org: &Org, e: &Executable) -> String {
    let svc = e.service.rsplit('.').next().unwrap_or(&e.service);
    let mut fields = String::new();
    for f in &e.inputs {
        fields.push_str(&format!("  {}{} {} = {};  // {}\n", if f.repeated { "repeated " } else { "" }, f.kind, f.name, f.number, f.doc));
    }
    format!(
        "// Generated by Muninn {ver} for {name} ({lang}, version {hash}) in {org}.\n\
         syntax = \"proto3\";\n\npackage {pkg};\n\nimport \"{COMMON_PROTO}\";\n\n\
         // What a caller may narrow ({inputs}).\nmessage {svc}Input {{\n{fields}}}\n\n\
         service {svc} {{\n  // Runs it; the view is {out}.\n  rpc Run({svc}Input) returns ({PACKAGE}.RunResult);\n}}\n",
        ver = env!("CARGO_PKG_VERSION"),
        name = e.name,
        lang = e.language,
        hash = &e.version_hash[..e.version_hash.len().min(16)],
        org = org.name,
        pkg = package(org),
        inputs = if e.inputs.is_empty() { "nothing: it reads no records".to_string() } else { "the records it reads".to_string() },
        out = e.output,
    )
}

// ---------- protobuf messages (as in common_proto) ----------

pub mod pb {
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Field {
        #[prost(string, tag = "1")]
        pub name: String,
        #[prost(uint32, tag = "2")]
        pub number: u32,
        #[prost(string, tag = "3")]
        pub r#type: String,
        #[prost(bool, tag = "4")]
        pub repeated: bool,
        #[prost(string, tag = "5")]
        pub doc: String,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Scope {
        #[prost(string, tag = "1")]
        pub kind: String,
        #[prost(string, tag = "2")]
        pub org_id: String,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Executable {
        #[prost(string, tag = "1")]
        pub id: String,
        #[prost(string, tag = "2")]
        pub name: String,
        #[prost(string, tag = "3")]
        pub service: String,
        #[prost(string, tag = "4")]
        pub language: String,
        #[prost(string, tag = "5")]
        pub version_hash: String,
        #[prost(message, optional, tag = "6")]
        pub scope: Option<Scope>,
        #[prost(message, repeated, tag = "7")]
        pub inputs: Vec<Field>,
        #[prost(string, tag = "8")]
        pub output: String,
        #[prost(string, tag = "9")]
        pub proto: String,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct ListRequest {
        #[prost(string, tag = "1")]
        pub org: String,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct ExecutableList {
        #[prost(message, repeated, tag = "1")]
        pub executables: Vec<Executable>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Row {
        #[prost(string, repeated, tag = "1")]
        pub cells: Vec<String>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Series {
        #[prost(string, tag = "1")]
        pub name: String,
        #[prost(double, repeated, tag = "2")]
        pub values: Vec<f64>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct View {
        #[prost(string, tag = "1")]
        pub kind: String,
        #[prost(string, tag = "2")]
        pub title: String,
        #[prost(string, repeated, tag = "3")]
        pub columns: Vec<String>,
        #[prost(message, repeated, tag = "4")]
        pub rows: Vec<Row>,
        #[prost(message, optional, tag = "5")]
        pub x: Option<Series>,
        #[prost(bool, tag = "6")]
        pub x_time: bool,
        #[prost(message, repeated, tag = "7")]
        pub series: Vec<Series>,
        #[prost(string, tag = "8")]
        pub text: String,
        #[prost(message, repeated, tag = "9")]
        pub tiles: Vec<View>,
        #[prost(string, tag = "10")]
        pub dir: String,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct RunResult {
        #[prost(string, tag = "1")]
        pub status: String,
        #[prost(string, tag = "2")]
        pub error: String,
        #[prost(message, optional, tag = "3")]
        pub view: Option<View>,
        #[prost(string, repeated, tag = "4")]
        pub logs: Vec<String>,
        #[prost(int64, tag = "5")]
        pub writes: i64,
        #[prost(int64, tag = "6")]
        pub ms: i64,
        #[prost(string, tag = "7")]
        pub json: String,
    }
}

impl Executable {
    pub fn to_pb(&self) -> pb::Executable {
        pb::Executable {
            id: self.id.clone(),
            name: self.name.clone(),
            service: self.service.clone(),
            language: self.language.clone(),
            version_hash: self.version_hash.clone(),
            scope: Some(pb::Scope { kind: self.scope.kind().into(), org_id: self.scope.org_id().unwrap_or_default().to_string() }),
            inputs: self
                .inputs
                .iter()
                .map(|f| pb::Field { name: f.name.into(), number: f.number, r#type: f.kind.into(), repeated: f.repeated, doc: f.doc.into() })
                .collect(),
            output: self.output.clone(),
            proto: self.proto.clone(),
        }
    }
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        o => o.to_string(),
    }
}

fn series(v: &Value) -> pb::Series {
    pb::Series {
        name: v["name"].as_str().unwrap_or_default().into(),
        values: v["values"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()).unwrap_or_default(),
    }
}

fn view_pb(v: &Value) -> pb::View {
    let strs = |k: &str| v[k].as_array().map(|a| a.iter().map(cell).collect()).unwrap_or_default();
    pb::View {
        kind: v["kind"].as_str().unwrap_or_default().into(),
        title: v["title"].as_str().unwrap_or_default().into(),
        columns: strs("columns"),
        rows: v["rows"]
            .as_array()
            .map(|rs| rs.iter().map(|r| pb::Row { cells: r.as_array().map(|c| c.iter().map(cell).collect()).unwrap_or_default() }).collect())
            .unwrap_or_default(),
        x: v.get("x").filter(|x| x.is_object()).map(series),
        x_time: v["x"]["time"].as_bool().unwrap_or(false),
        series: v["series"].as_array().map(|a| a.iter().map(series).collect()).unwrap_or_default(),
        text: v["text"].as_str().unwrap_or_default().into(),
        tiles: v["tiles"].as_array().map(|a| a.iter().map(view_pb).collect()).unwrap_or_default(),
        dir: v["dir"].as_str().unwrap_or_default().into(),
    }
}

/// A run's JSON as a `RunResult`.
pub fn result_pb(v: &Value) -> pb::RunResult {
    pb::RunResult {
        status: v["status"].as_str().unwrap_or_default().into(),
        error: v["error"].as_str().unwrap_or_default().into(),
        view: v.get("view").filter(|x| x.is_object()).map(view_pb),
        logs: v["logs"].as_array().map(|a| a.iter().map(cell).collect()).unwrap_or_default(),
        writes: v["writes"].as_i64().unwrap_or(0),
        ms: v["ms"].as_i64().unwrap_or(0),
        json: v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{CodeInput, Lang};

    #[test]
    fn names_inputs_and_outputs() {
        assert_eq!(pascal("sma cross-2"), "SmaCross2");
        assert_eq!(pascal("9 lives"), "X9Lives");
        let s = Store::open_in_memory().unwrap();
        let o = super::super::create(&s, "cli", "Comet-Lab 2", None, "").unwrap();
        assert_eq!(package(&o), "muninn.code.v1.o.comet_lab_2");
        let c =
            s.create_code_space("sma", "", Lang::Math, "@view table\nx = close", CodeInput { model: "bars".into(), ..Default::default() }).unwrap();
        assert_eq!(output_kind(&c), "table");
        assert_eq!(inputs(&c).len(), 5);
        let none = s.create_code_space("const", "", Lang::Math, "x = 1", CodeInput::default()).unwrap();
        assert!(inputs(&none).is_empty() && output_kind(&none) == "any");
        // narrowing
        let mut run = c.clone();
        let i = Input::from_json(&json!({ "where": "close > 3", "limit": 10 })).unwrap();
        i.apply(&mut run).unwrap();
        assert_eq!((run.input.filter.clone(), run.input.limit), (vec!["close > 3".to_string()], Some(10)));
        assert!(Input::from_json(&json!({ "drop": 1 })).is_err());
        let mut n = none.clone();
        assert!(i.apply(&mut n).is_err(), "no input to narrow");
    }

    #[test]
    fn only_served_spaces_the_caller_may_run_are_listed() {
        let s = Store::open_in_memory().unwrap();
        let (o, ben) = super::super::tests::org_with_member(&s, "viewer");
        let c = s.create_code_space("sma", "", Lang::Math, "x = close", CodeInput::default()).unwrap();
        assert!(executables(&s, &o, &Actor::Owner).unwrap().is_empty(), "not served");
        s.serve_code(&o.id, &c.id).unwrap();
        let es = executables(&s, &o, &Actor::Owner).unwrap();
        assert_eq!((es.len(), es[0].service.as_str()), (1, "muninn.code.v1.o.lab.Sma"));
        assert!(es[0].version_hash.len() >= 16);
        let viewer = super::super::actor_for(&s, &ben).unwrap();
        assert!(executables(&s, &o, &viewer).unwrap().is_empty(), "viewers cannot run code");
        assert!(run(&s, "cli", &viewer, &o, "sma", &Input::default()).is_err());
        let err = run(&s, "cli", &Actor::Owner, &o, "Sma", &Input::default()).unwrap_err().to_string();
        assert!(err.contains("approve"), "{err}");
        let p = space_proto(&o, &es[0]);
        assert!(p.contains("package muninn.code.v1.o.lab;") && p.contains("service Sma {") && p.contains("rpc Run(SmaInput)"), "{p}");
        assert!(common_proto().contains("message Executable {"));
        let r = result_pb(&json!({ "status": "ok", "view": { "kind": "table", "columns": ["a"], "rows": [[1.5], [null]] }, "ms": 3 }));
        let v = r.view.unwrap();
        assert_eq!((v.kind.as_str(), v.rows[0].cells[0].as_str(), v.rows[1].cells[0].as_str(), r.ms), ("table", "1.5", "", 3));
        assert_eq!(es[0].to_pb().scope.unwrap().kind, "user");
    }
}
